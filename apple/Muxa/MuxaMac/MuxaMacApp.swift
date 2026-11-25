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
                .modifier(ThemedWindow())
                .frame(minWidth: 720, minHeight: 480)
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
                        await KeyCheck.runIfAsked(model: model, nav: nav, chrome: chrome)
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
                .modifier(ThemedWindow())
        }
        .commands {
            // The items are views: a View body observes the model and the
            // navigation, so the items enable and disable as they change. Read
            // straight in `commands`, they kept the state of the first launch.
            CommandGroup(replacing: .saveItem) {
                CloseItem(model: model, nav: nav, chrome: chrome)
            }
            CommandGroup(replacing: .newItem) {
                CaptureItems(model: model, nav: nav) { quickCapture?.show() }
            }
            // The keys of docs/next.md (ADR 0070 decision 2), which the user can change (ADR 0078).
            CommandMenu("Go") {
                GoMenuItems(model: model, nav: nav)
            }
            CommandMenu("Pane") {
                PaneMenuItems(model: model, nav: nav, chrome: chrome)
            }
            CommandMenu("Host") {
                HostMenuItems(model: model)
            }
            CommandGroup(replacing: .help) {
                Button("Keyboard Shortcuts") { chrome.showShortcuts = true }
                    .shortcut(.shortcuts)
            }
        }
    }
}

/// The app keeps running with its window closed: `⌃⌥Space` still opens
/// Capture, and Capture's `⌘⏎` opens the window again.
final class AppDelegate: NSObject, NSApplicationDelegate {
    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool { false }

    #if DEBUG
    func applicationDidFinishLaunching(_ notification: Notification) {
        ReaderCheck.runIfAsked()
    }
    #endif
}

/// Runs `start` when the main window appears, with the action that reopens it.
private struct OnWindowStart: ViewModifier {
    @Environment(\.openWindow) private var openWindow
    let start: (OpenWindowAction) async -> Void

    func body(content: Content) -> some View {
        content.task { await start(openWindow) }
    }
}
