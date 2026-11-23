import AppKit
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
    @State private var windowKeys: WindowKeyMonitor?

    init() {
        TerminalSettings.registerFonts()
    }

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
                .background(WindowReader { chrome.mainWindow = $0 })
                // Every screen draws the mocks' header where the title bar was.
                .toolbar(.hidden, for: .windowToolbar)
                .ignoresSafeArea(.container, edges: .top)
                .modifier(OnWindowStart { openWindow in
                    nav.openElsewhere = { [weak chrome, weak nav] t in
                        guard let chrome, let nav else { return false }
                        return chrome.showOnBotPage(t, nav: nav)
                    }
                    let capture = quickCapture ?? QuickCapture(model: model, nav: nav)
                    capture.register()
                    capture.openMain = { openWindow(id: "main") }
                    quickCapture = capture
                    chrome.showCapture = { [weak capture] in capture?.show() }
                    let monitor = windowKeys ?? WindowKeyMonitor(model: model, nav: nav, chrome: chrome)
                    monitor.install()
                    windowKeys = monitor
                    // A reopened window finds the host already connected.
                    guard case .offline = model.link else { return }
                    if ProcessInfo.processInfo.arguments.contains("-demo") {
                        await model.startDemo()
                        #if DEBUG
                        await Snapshot.runIfAsked(model: model, nav: nav, chrome: chrome)
                        #endif
                    } else if model.prefersLocal, model.serverIsLocal {
                        await model.connectLocal()
                    } else if let last = model.hosts.lastUsed {
                        await model.connect(to: last)
                    }
                })
        }
        // The screens draw the mocks' own header bar.
        .windowStyle(.hiddenTitleBar)
        // ⌘, opens it, as in every Mac app.
        Settings {
            MacSettingsView()
                .environment(model)
                .environment(nav)
                .environment(chrome)
                .frame(minWidth: 960, minHeight: 600)
                .background(Palette.ground)
                .preferredColorScheme(.dark)
        }
        .commands {
            CommandGroup(replacing: .saveItem) {
                Button(PaneCommands.activeShell(nav: nav, model: model) != nil ? "Close Shell" : "Close") { close() }
                    .shortcut(.closePane)
            }
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
            // The keys of docs/next.md (ADR 0070 decision 2), which the user can change (ADR 0078).
            CommandMenu("Go") {
                Button("Home") { nav.focus = false; nav.tab = .inbox }
                    .shortcut(.home)
                Button("Focus") { nav.focus = true }
                    .shortcut(.focus)
                Button("Bots") { nav.focus = false; nav.tab = .bots }
                    .shortcut(.bots)
                    .disabled(!model.isConnected)
                Button("Routines in Bots") { nav.focus = false; nav.tab = .bots }
                    .shortcut(.routines)
                    .disabled(!model.isConnected)
                Divider()
                Button("Next Task That Needs You") { openNext(in: model.needsYou) }
                    .shortcut(.nextNeedsYou)
                    .disabled(model.needsYou.isEmpty)
                Button("Next Task") { openNext(in: model.needsYou + model.liveTasks) }
                    .shortcut(.nextTask)
                    .disabled(model.liveTasks.isEmpty && model.needsYou.isEmpty)
                Button("Previous Task That Needs You") { openNext(in: model.needsYou, step: -1) }
                    .shortcut(.previousNeedsYou)
                    .disabled(model.needsYou.isEmpty)
                Button("Previous Task") { openNext(in: model.needsYou + model.liveTasks, step: -1) }
                    .shortcut(.previousTask)
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
                // The keys work in Focus only, so Home keeps them for its own use.
                Button("Split Pane") { pane(.split) }
                    .shortcut(.split)
                    .disabled(!PaneCommands.canSplit(nav: nav, model: model))
                Button("Next Pane") { pane(.next) }
                    .shortcut(.nextPane)
                    .disabled(!nav.focus || nav.panes.count < 2)
                Button("Previous Pane") { pane(.previous) }
                    .shortcut(.previousPane)
                    .disabled(!nav.focus || nav.panes.count < 2)
                Button("Close Other Shells") { pane(.closeOthers) }
                    .shortcut(.closeOtherPanes)
                    .disabled(!nav.focus || nav.panes.count < 2)
                Divider()
                Button("Close Task…") { chrome.closingTask = focusedTask }
                    .shortcut(.closeTask)
                    .disabled(focusedTask == nil)
                Divider()
                // In Bots the first four keys pick the Bot's tab instead (ADR 0086).
                ForEach(1...9, id: \.self) { n in
                    Button(inBots ? (BotTab(rawValue: n)?.title ?? "Pane \(n)") : "Pane \(n)") {
                        if inBots, let tab = BotTab(rawValue: n) { chrome.botTab = tab } else { pane(.pane(n)) }
                    }
                    .keyboardShortcut(KeyEquivalent(Character("\(n)")), modifiers: .command)
                    .disabled(inBots ? BotTab(rawValue: n) == nil : n > nav.panes.count)
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

    /// The Bots page shows: `⌘1`…`⌘4` are its tabs.
    private var inBots: Bool { !nav.focus && (nav.tab == .bots || nav.tab == .routines) }

    private func pane(_ command: WindowCommand) {
        PaneCommands.run(command, nav: nav, model: model)
    }

    /// The Task Focus shows, while its agent runs.
    private var focusedTask: Terminal? {
        guard nav.focus else { return nil }
        return model.terminals.first { $0.id == nav.terminal && $0.isTaskAgent }
    }

    /// ⌘W: in Focus it closes the active shell and never the window, so a
    /// stray ⌘W on the agent does nothing. Elsewhere it closes the key window.
    private func close() {
        let key = NSApp.keyWindow
        guard nav.focus, let key, key === chrome.mainWindow else { return key?.performClose(nil) ?? () }
        if PaneCommands.activeShell(nav: nav, model: model) != nil {
            pane(.close)
        } else {
            NSSound.beep()
        }
    }

    /// Opens the Task after (or with `step` -1, before) the one Focus shows,
    /// from `list`, in Focus. It wraps around, so each press moves on.
    private func openNext(in list: [Terminal], step: Int = 1) {
        let unique = list.reduce(into: [Terminal]()) { if !$0.contains($1) { $0.append($1) } }
        guard !unique.isEmpty else { return }
        let at = unique.firstIndex { $0.id == nav.terminal } ?? (step > 0 ? -1 : 0)
        nav.open(unique[(at + step + unique.count) % unique.count])
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
