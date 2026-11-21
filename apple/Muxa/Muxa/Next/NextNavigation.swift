import MuxaV2
import Observation
import SwiftUI

/// Where the user is: Home or Focus, the Task shown, and Focus's panels.
/// Views and the Mac's menu commands share it.
@MainActor
@Observable
final class NextNavigation {
    var focus = false
    /// The terminal Focus shows.
    var terminal: String?
    var showInbox = false
    var sidebar = NavigationSplitViewVisibility.all

    func open(_ t: MuxaV2.Terminal) {
        terminal = t.id
        focus = true
    }

    func toggleSidebar() {
        sidebar = sidebar == .detailOnly ? .all : .detailOnly
    }
}
