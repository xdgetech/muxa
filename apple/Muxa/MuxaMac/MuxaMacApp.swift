import MuxaSession
import MuxaV2
import SwiftUI

/// The desktop app (ADR 0070): Home and Focus on a host's muxa server over
/// protocol v2, as the iOS app. `-demo` starts the in-app demo server (UI tests).
@main
struct MuxaMacApp: App {
    @NSApplicationDelegateAdaptor private var delegate: AppDelegate
    @State private var model = NextModel(hosts: HostStore(file: HostStore.defaultFile))
    @State private var nav = NextNavigation()
    @State private var quickCapture: QuickCapture?
    @State private var chrome = MacChrome()

    var body: some Scene {
        Window("Muxa", id: "main") {
            NextRoot()
                .environment(model)
                .environment(nav)
                .environment(chrome)
                .tint(Theme.accent)
                .frame(minWidth: 720, minHeight: 480)
                .containerBackground(Palette.ground, for: .window)
                .toolbarBackground(Palette.ground, for: .windowToolbar)
                .sheet(isPresented: $chrome.showShortcuts) { ShortcutsSheet() }
                .modifier(OnWindowStart { openWindow in
                    let capture = quickCapture ?? QuickCapture(model: model, nav: nav)
                    capture.register()
                    capture.openMain = { openWindow(id: "main") }
                    quickCapture = capture
                    chrome.showCapture = { [weak capture] in capture?.show() }
                    // A reopened window finds the host already connected.
                    guard case .offline = model.link else { return }
                    if ProcessInfo.processInfo.arguments.contains("-demo") {
                        await model.startDemo()
                        #if DEBUG
                        await Snapshot.runIfAsked(nav: nav, chrome: chrome)
                        #endif
                    } else if model.prefersLocal, model.serverIsLocal {
                        await model.connectLocal()
                    } else if let last = model.hosts.lastUsed {
                        await model.connect(to: last)
                    }
                })
        }
        // ⌘, opens it, as in every Mac app.
        Settings {
            SettingsStack()
                .environment(model)
                .environment(nav)
                .environment(chrome)
                .frame(minWidth: 640, minHeight: 560)
                .background(Palette.ground)
                .preferredColorScheme(.dark)
        }
        .commands {
            CommandGroup(replacing: .newItem) {
                Button("Capture…") { quickCapture?.show() }
                    .shortcut(.capture)
                    .disabled(!model.isConnected)
                Button("Capture from Anywhere") { quickCapture?.show() }
                    .shortcut(.captureAnywhere)
                Button("New Task…") { nav.newTask = TaskSeed() }
                    .shortcut(.newTask)
                    .disabled(!model.isConnected)
            }
            // The keys of docs/next.md, which follow VS Code (ADR 0070 decision 2).
            CommandMenu("Go") {
                Button("Home") { nav.focus = false; nav.tab = .inbox }
                    .shortcut(.home)
                Button("Focus") { nav.focus = true }
                    .shortcut(.focus)
                Button("Routines") { nav.focus = false; nav.tab = .routines }
                    .shortcut(.routines)
                    .disabled(!model.isConnected)
                Divider()
                Button("Next Task That Needs You") { openNext(in: model.needsYou) }
                    .shortcut(.nextNeedsYou)
                    .disabled(model.needsYou.isEmpty)
                Button("Next Task") { openNext(in: model.needsYou + model.liveTasks) }
                    .shortcut(.nextTask)
                    .disabled(model.liveTasks.isEmpty && model.needsYou.isEmpty)
                Divider()
                Button(nav.showInbox ? "Hide Inbox Panel" : "Show Inbox Panel") { nav.showInbox.toggle() }
                    .shortcut(.inboxPanel)
                    .disabled(!nav.focus)
                Button(nav.showReader ? "Hide Reader Panel" : "Show Reader Panel") { nav.showReader.toggle() }
                    .shortcut(.readerPanel)
                    .disabled(!nav.focus)
                Button(nav.showChat ? "Hide Chat Panel" : "Show Chat Panel") { nav.showChat.toggle() }
                    .shortcut(.chatPanel)
                    .disabled(!nav.focus)
                Button("Toggle Sidebar") { nav.toggleSidebar() }
                    .shortcut(.sidebar)
                    .disabled(!nav.focus)
                Divider()
                Button("Go to Task…") { nav.showPalette = true }
                    .shortcut(.goToTask)
                    .disabled(!model.isConnected)
            }
            CommandMenu("Pane") {
                Button("Split Pane") {
                    nav.focus = true
                    nav.split(from: model.needsYou + model.liveTasks + model.otherTerminals)
                }
                .shortcut(.split)
                .disabled(!model.isConnected || nav.panes.count >= NextNavigation.maxPanes)
                Button("Close Pane") { nav.closePane() }
                    .disabled(nav.panes.count < 2)
                Divider()
                ForEach(1...9, id: \.self) { n in
                    Button("Pane \(n)") { nav.focusPane(n) }
                        .keyboardShortcut(KeyEquivalent(Character("\(n)")), modifiers: .command)
                        .disabled(n > nav.panes.count)
                }
            }
            CommandMenu("Host") {
                Button("Disconnect") { Task { await model.disconnect() } }
                    .disabled(!model.isConnected)
            }
            CommandGroup(replacing: .help) {
                Button("Keyboard Shortcuts") { chrome.showShortcuts = true }
                    .shortcut(.shortcuts)
            }
        }
    }

    /// Opens the Task after the one Focus shows, from `list`, in Focus.
    private func openNext(in list: [Terminal]) {
        guard !list.isEmpty else { return }
        let at = list.firstIndex { $0.id == nav.terminal } ?? -1
        nav.open(list[(at + 1) % list.count])
    }
}

/// The app keeps running with its window closed: `⌃⌥Space` still opens
/// Capture, and Capture's `⌘⏎` opens the window again.
final class AppDelegate: NSObject, NSApplicationDelegate {
    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool { false }
}

/// Runs `start` when the main window appears, with the action that reopens it.
private struct OnWindowStart: ViewModifier {
    @Environment(\.openWindow) private var openWindow
    let start: (OpenWindowAction) async -> Void

    func body(content: Content) -> some View {
        content.task { await start(openWindow) }
    }
}
