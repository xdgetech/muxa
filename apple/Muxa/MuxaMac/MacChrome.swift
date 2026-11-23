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
    /// Settings opens on this Routine's editor; Settings clears it.
    var editRoutine: RoutineRef?
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
