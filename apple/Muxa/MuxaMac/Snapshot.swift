#if DEBUG
import AppKit
import SwiftUI

/// Debug builds only: with `-demo`, `MUXA_SNAPSHOT=<png>` and optionally
/// `MUXA_THEME=<theme id>` and
/// `MUXA_SCREEN=home|focus|routines|shortcuts|palette|reader|settings[-tab]|settings-terminal|settings-assistant|bots[-chat|-chats|-run|-routines|-files|-settings|-new|-routine-edit|-soul-edit]` in the environment, the app
/// draws its main window into the PNG and quits, so a change can be compared
/// with the mocks without screen-recording permission. Environment, not
/// arguments: macOS opens a path argument as a document and shows no window.
@MainActor
enum Snapshot {
    static func runIfAsked(model: NextModel, nav: NextNavigation, chrome: MacChrome) async {
        let env = ProcessInfo.processInfo.environment
        guard let path = env["MUXA_SNAPSHOT"] else { return }
        if let theme = env["MUXA_THEME"] { ThemeStore.shared.preview(theme) }
        switch env["MUXA_SCREEN"] {
        case "focus": nav.focus = true
        case "routines": nav.tab = .bots
        case "bots-landing-home", "bots-landing-chat":
            // The landing rule picks the tab: scout has a live talk, pr-watch none.
            nav.tab = .bots
            chrome.botPick = .bot(workspace: "app", name: env["MUXA_SCREEN"] == "bots-landing-chat" ? "scout" : "pr-watch")
        case let screen? where screen.hasPrefix("bots"):
            nav.tab = .bots
            chrome.botPick = .bot(workspace: "app", name: "pr-watch")
            let tabs: [String: BotTab] = ["bots-chat": .chats, "bots-run": .chats, "bots-routines": .routines,
                                          "bots-files": .files, "bots-settings": .settings, "bots-routine-edit": .routines, "bots-soul-edit": .files]
            chrome.botTab = tabs[screen] ?? .home
            chrome.botTabs[.bot(workspace: "app", name: "pr-watch")] = chrome.botTab
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
