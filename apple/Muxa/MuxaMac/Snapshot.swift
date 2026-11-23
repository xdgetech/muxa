#if DEBUG
import AppKit
import SwiftUI

/// Debug builds only: with `-demo`, `MUXA_SNAPSHOT=<png>` and optionally
/// `MUXA_SCREEN=home|focus|routines|shortcuts|settings[-tab]|settings-terminal|palette|reader|settings-assistant|focus-talk|bots[-chats|-routines|-files|-new]` in the environment, the app
/// draws its main window into the PNG and quits, so a change can be compared
/// with the mocks without screen-recording permission. Environment, not
/// arguments: macOS opens a path argument as a document and shows no window.
@MainActor
enum Snapshot {
    static func runIfAsked(nav: NextNavigation, chrome: MacChrome) async {
        let env = ProcessInfo.processInfo.environment
        guard let path = env["MUXA_SNAPSHOT"] else { return }
        switch env["MUXA_SCREEN"] {
        case "focus": nav.focus = true
        case "routines": nav.tab = .bots
        case let screen? where screen.hasPrefix("bots"):
            nav.tab = .bots
            chrome.botPick = .bot(workspace: "app", name: "pr-watch")
            let tabs: [String: BotTab] = ["bots-chats": .chats, "bots-routines": .routines, "bots-files": .files]
            chrome.botTab = tabs[screen] ?? .home
            chrome.showNewBot = screen == "bots-new"
        case "focus-talk":
            nav.terminal = "t7"
            nav.focus = true
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
