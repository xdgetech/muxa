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
    /// Settings opens on this Routine's editor; Settings clears it.
    var editRoutine: RoutineRef?
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
        botTerminal = t.id
        botView = t.isTaskAgent ? .chat : .terminal
        return true
    }
}

/// How the Chats tab shows its open terminal. Chat serves Task agents only.
enum BotView: Hashable {
    case chat, terminal, reader
}

/// What the Bots list picked: a Bot of a Workspace, or a Routine without a Bot.
enum BotPick: Hashable {
    case bot(workspace: String, name: String)
    case routine(id: String)
}

/// The tabs of a Bot's page; `⌘1`…`⌘4` pick them in Bots.
enum BotTab: Int, CaseIterable {
    case home = 1, chats, routines, files

    var title: String {
        switch self {
        case .home: "Home"
        case .chats: "Chats"
        case .routines: "Routines"
        case .files: "Files"
        }
    }
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
