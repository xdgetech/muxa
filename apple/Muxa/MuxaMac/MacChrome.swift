import AppKit
import MuxaV2
import Observation
import SwiftUI

/// What the Mac's menus and its screens share beyond navigation: the
/// shortcuts sheet and the Quick Capture panel. The app sets `showCapture`.
@MainActor
@Observable
final class MacChrome {
    var showShortcuts = false
    /// The Task the user asked to close; Focus confirms it first.
    var closingTask: MuxaV2.Terminal?
    /// The main window, so ⌘W knows when Focus has the keyboard.
    weak var mainWindow: NSWindow?
    /// Vim's window prefix was pressed and waits for its command key.
    var windowPrefixPending = false
    /// Opens the Quick Capture panel.
    var showCapture: () -> Void = {}
    /// The Bots page's pick and the picked Bot's tab (ADR 0086).
    var botPick: BotPick?
    var botTab = BotTab.home
    var showNewBot = false
    /// The talk, run or member open on the Chats tab, and how it shows.
    var botTerminal: String?
    var botView = BotView.chat
    /// The tab each Bot was left on in this run; a Bot not in it opens by the landing rule.
    var botTabs: [BotPick: BotTab] = [:]
    /// The Routine or Soul editor open over the Bot page.
    var botSheet: BotSheet?
}

extension MacChrome {
    /// Opens a Bot's terminal on its Bot's page (ADR 0086): a talk on Chat, a
    /// run or a member on Terminal, where the user answers its menu. False
    /// for a terminal of no Bot, which opens in Focus.
    func showOnBotPage(_ t: MuxaV2.Terminal, nav: NextNavigation) -> Bool {
        guard t.belongsToBot, let workspace = t.labels["muxa.workspace"], let bot = t.bot ?? t.lead else { return false }
        nav.focus = false
        nav.tab = .bots
        botPick = .bot(workspace: workspace, name: bot)
        botTab = .chats
        botTabs[.bot(workspace: workspace, name: bot)] = .chats
        botTerminal = t.id
        botView = t.isTaskAgent ? .chat : .terminal
        return true
    }
}

/// An editor Settings also uses, as a sheet over the Bot page: the user stays
/// where the Routine or the Bot is.
enum BotSheet: Identifiable {
    case routine(workspace: String, draft: RoutineDraft)
    case soul(workspace: SettingsWorkspace, bot: SettingsBot)

    var id: String {
        switch self {
        case let .routine(w, d): "routine.\(w).\(d.id)"
        case let .soul(w, b): "soul.\(w.name).\(b.name)"
        }
    }
}

extension MacChrome {
    /// A Bot's page opens on the tab it was left on in this run. The first
    /// time, on Chats with its live talk if it has one, else on Home (ADR 0086).
    func land(on pick: BotPick, liveTalk: MuxaV2.Terminal?) {
        if let left = botTabs[pick] {
            botTab = left
            return
        }
        if let talk = liveTalk {
            botTab = .chats
            botTerminal = talk.id
            botView = .chat
        } else {
            botTab = .home
        }
        botTabs[pick] = botTab
    }
}

/// How the Chats tab shows its open terminal. Chat serves Task agents only.
enum BotView: Hashable {
    case chat, terminal, reader
}

/// Hands the window a view sits in to `found`.
struct WindowReader: NSViewRepresentable {
    let found: (NSWindow?) -> Void

    func makeNSView(context: Context) -> NSView {
        let view = NSView()
        DispatchQueue.main.async { [weak view] in found(view?.window) }
        return view
    }

    func updateNSView(_ view: NSView, context: Context) {}
}
