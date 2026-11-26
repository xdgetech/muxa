#if DEBUG
import AppKit
import SwiftUI

/// Debug builds only: with `-demo`, `MUXA_SNAPSHOT=<png>` and optionally
/// `MUXA_THEME=<theme id>` and
/// `MUXA_SCREEN=home|focus|routines|shortcuts|palette|reader|settings[-tab]|settings-terminal|settings-assistant|bots[-chat|-chats|-run|-routines|-files|-settings|-new|-routine-edit|-soul-edit|-landing-chat|-landing-done|-landing-left-home|-landing-run]|focus-tab-hidden|focus-select|focus-link` in the environment, the app
/// draws its main window into the PNG and quits, so a change can be compared
/// with the mocks without screen-recording permission. `focus-select` and
/// `focus-link` Option-drag over, or ⌘-hover, the URL in `MUXA_TERMINAL`
/// (default `t1`, which takes the mouse as Codex does). Environment, not
/// arguments: macOS opens a path argument as a document and shows no window.
@MainActor
enum Snapshot {
    static func runIfAsked(model: NextModel, nav: NextNavigation, chrome: MacChrome) async {
        let env = ProcessInfo.processInfo.environment
        guard let path = env["MUXA_SNAPSHOT"] else { return }
        if let theme = env["MUXA_THEME"] { ThemeStore.shared.preview(theme) }
        switch env["MUXA_SCREEN"] {
        case "focus": nav.focus = true
        case "focus-select", "focus-link":
            nav.focus = true
            nav.terminal = env["MUXA_TERMINAL"] ?? "t1"
        case "routines": nav.tab = .bots
        case "bots-landing-chat":
            // The landing rule picks the tab: scout has a live talk.
            nav.tab = .bots
            chrome.botPick = .bot(workspace: "app", name: "scout")
        case "bots-landing-done", "bots-landing-left-home":
            // pr-watch's talk reported `done`; "left-home" was last left on Home. Both land on the talk.
            nav.tab = .bots
            chrome.botTab = .home
            if env["MUXA_SCREEN"] == "bots-landing-left-home" { chrome.botTabs[.bot(workspace: "app", name: "pr-watch")] = .home }
            chrome.botPick = .bot(workspace: "app", name: "pr-watch")
        case "bots-landing-run":
            // An Inbox item's run opens on its Bot's page and stays there, not on the talk.
            for _ in 0..<30 where model.terminals.isEmpty { try? await Task.sleep(for: .milliseconds(100)) }
            if let run = model.terminals.first(where: { $0.id == "t8" }) { _ = chrome.showOnBotPage(run, nav: nav) }
        case "focus-tab-hidden":
            // t1's tab was closed: t1 still runs in the sidebar, t2 is open.
            nav.focus = true
            nav.terminal = "t2"
            chrome.hiddenTabs = ["t1"]
        case let screen? where screen.hasPrefix("bots"):
            nav.tab = .bots
            chrome.botPick = .bot(workspace: "app", name: "pr-watch")
            let tabs: [String: BotTab] = ["bots-chat": .chats, "bots-run": .chats, "bots-routines": .routines,
                                          "bots-files": .files, "bots-settings": .settings, "bots-routine-edit": .routines, "bots-soul-edit": .files]
            chrome.botTab = tabs[screen] ?? .home
            chrome.botTabs[.bot(workspace: "app", name: "pr-watch")] = chrome.botTab
            // The screen picks the tab, not the landing rule.
            chrome.botRoutedTo = .bot(workspace: "app", name: "pr-watch")
            chrome.botTerminal = screen == "bots-run" ? "t8" : "t7"
            chrome.botView = screen == "bots-run" ? .terminal : .chat
            chrome.showNewBot = screen == "bots-new"
            if screen.hasSuffix("-edit") { await openEditor(screen, model: model, chrome: chrome) }
        case "shortcuts": chrome.showShortcuts = true
        case "palette":
            nav.focus = true
            nav.showPalette = true
        case "reader":
            nav.focus = true
            nav.showReader = true
        default: break
        }
        // The mocks' desktop frame.
        NSApp.windows.first { $0.isVisible && !($0 is NSPanel) }?.setContentSize(NSSize(width: 1440, height: 900))
        let settings = await openSettings(env["MUXA_SCREEN"])
        try? await Task.sleep(for: .seconds(2))
        if let screen = env["MUXA_SCREEN"], screen == "focus-select" || screen == "focus-link" {
            mouseOverURL(select: screen == "focus-select")
            try? await Task.sleep(for: .milliseconds(500))
        }
        let window = settings ?? NSApp.windows.first { $0.attachedSheet != nil }
            ?? NSApp.windows.first { $0.isVisible && !($0 is NSPanel) }
        let target = window?.attachedSheet ?? window
        if let view = target?.contentView?.superview ?? target?.contentView,
           let rep = view.bitmapImageRepForCachingDisplay(in: view.bounds) {
            view.cacheDisplay(in: view.bounds, to: rep)
            try? rep.representation(using: .png, properties: [:])?.write(to: URL(fileURLWithPath: path))
        }
        exit(0)
    }

    /// pr-watch's `scan` or its Soul in the editor over the Bot page, once the settings are read.
    private static func openEditor(_ screen: String, model: NextModel, chrome: MacChrome) async {
        for _ in 0..<30 where model.settings.workspace("app") == nil { try? await Task.sleep(for: .milliseconds(100)) }
        guard let app = model.settings.workspace("app") else { return }
        if screen == "bots-soul-edit", let bot = app.bots.first(where: { $0.name == "pr-watch" }) {
            chrome.botSheet = .soul(workspace: app, bot: bot)
        } else if let scan = app.routines.first(where: { $0.name == "scan" }) {
            chrome.botSheet = .routine(workspace: "app", draft: RoutineDraft(scan))
        }
    }

    /// Option-drags across the pane's first URL, or ⌘-hovers it, with synthetic events.
    private static func mouseOverURL(select: Bool) {
        guard let window = NSApp.windows.first(where: { $0.isVisible && !($0 is NSPanel) }),
              let pane = window.contentView.flatMap(firstPane) else { return }
        let lines = String(decoding: pane.getBufferAsData(kind: .active), as: UTF8.self).components(separatedBy: "\n")
        guard let row = lines.firstIndex(where: { $0.contains("https://") }),
              let start = lines[row].range(of: "https://") else { return }
        let col = lines[row].distance(from: lines[row].startIndex, to: start.lowerBound)
        let length = lines[row][start.lowerBound...].prefix { $0 != " " }.count
        let rows = CGFloat(pane.terminalDimensions.rows), cols = CGFloat(pane.terminalDimensions.cols)
        func event(_ type: NSEvent.EventType, _ c: Int, _ flags: NSEvent.ModifierFlags) -> NSEvent? {
            let x = (CGFloat(c) + 0.5) * pane.bounds.width / cols
            let y = pane.bounds.height - (CGFloat(row) + 0.5) * pane.bounds.height / rows
            let point = pane.convert(NSPoint(x: x, y: y), to: nil)
            if type == .flagsChanged {
                return NSEvent.keyEvent(with: type, location: point, modifierFlags: flags, timestamp: 0,
                                        windowNumber: window.windowNumber, context: nil, characters: "",
                                        charactersIgnoringModifiers: "", isARepeat: false, keyCode: 55)
            }
            return NSEvent.mouseEvent(with: type, location: point, modifierFlags: flags, timestamp: 0,
                                      windowNumber: window.windowNumber, context: nil, eventNumber: 0,
                                      clickCount: 1, pressure: 1)
        }
        if select {
            event(.leftMouseDown, col, .option).map(pane.mouseDown)
            event(.leftMouseDragged, col + length - 1, .option).map(pane.mouseDragged)
            event(.leftMouseUp, col + length - 1, .option).map(pane.mouseUp)
        } else {
            event(.flagsChanged, col, .command).map(pane.flagsChanged)
            event(.mouseMoved, col + 2, .command).map(pane.mouseMoved)
        }
    }

    private static func firstPane(in view: NSView) -> MacPaneView? {
        if let pane = view as? MacPaneView { return pane }
        return view.subviews.lazy.compactMap(firstPane).first
    }

    /// `settings` or `settings-<tab>` (connections, mcp, agent, routines, bots):
    /// opens the Settings window at the mocks' size and returns it.
    private static func openSettings(_ screen: String?) async -> NSWindow? {
        guard let screen, screen.hasPrefix("settings") else { return nil }
        let tabs: [String: WorkspaceSettings.Tab] = [
            "settings-connections": .connections, "settings-mcp": .mcp, "settings-agent": .agent, "settings-routines": .routines,
            "settings-bots": .bots,
        ]
        MacWorkspacePane.firstTab = tabs[screen] ?? .general
        if screen == "settings-terminal" { MacSettingsView.firstPlace = .terminal }
        if screen == "settings-assistant" { MacSettingsView.firstPlace = .assistant }
        let before = Set(NSApp.windows.map(ObjectIdentifier.init))
        // The app menu's Settings… item: a nil-targeted action finds no
        // responder while the app is in the background.
        if let menu = NSApp.mainMenu?.items.first?.submenu,
           let at = menu.items.firstIndex(where: { $0.keyEquivalent == "," }) {
            menu.performActionForItem(at: at)
        }
        try? await Task.sleep(for: .seconds(1))
        let window = NSApp.windows.first { $0.isVisible && !before.contains(ObjectIdentifier($0)) } ?? NSApp.keyWindow
        window?.setContentSize(NSSize(width: 1440, height: 900))
        return window
    }
}
#endif
