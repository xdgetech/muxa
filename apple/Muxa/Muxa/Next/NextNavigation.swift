import MuxaV2
import Observation
import SwiftUI

/// Where the user is: Home or Focus, the panes Focus shows, and its panels.
/// Views and the Mac's menu commands share it.
@MainActor
@Observable
final class NextNavigation {
    /// The touch app's main parts: the iPhone's tabs, the iPad sidebar's segments.
    /// `bots` is the Mac's Bots page (ADR 0086).
    enum Tab: Hashable { case inbox, tasks, routines, hosts, bots }

    /// A Task is open: Focus on the Mac, the Task screen on a phone, and the
    /// sidebar's Tasks on an iPad.
    var focus = false
    var tab = Tab.inbox
    /// The open Task's terminal, then its shells (ADR 0079): the Mac shows
    /// them side by side, iOS one at a time.
    private(set) var panes: [String] = []
    /// The pane that has the keyboard.
    private(set) var active = 0
    /// Bumped to move the keyboard to the active pane.
    private(set) var focusRequest = 0
    /// The panel beside Focus shows one of the Inbox, the Reader and Chat.
    var showInbox = false {
        didSet { if showInbox { showReader = false; showChat = false } }
    }
    var showReader = false {
        didSet { if showReader { showInbox = false; showChat = false } }
    }
    var showChat = false {
        didSet { if showChat { showInbox = false; showReader = false } }
    }
    /// What a touch Task screen shows: its segments are the Chat and Reader panels.
    enum TaskView: Hashable { case chat, terminal, reader }

    var taskView: TaskView {
        get { showChat ? .chat : showReader ? .reader : .terminal }
        set {
            switch newValue {
            case .chat: showChat = true
            case .reader: showReader = true
            case .terminal:
                showChat = false
                showReader = false
            }
        }
    }

    var showPalette = false
    /// The Start Task sheet is open with this seed.
    var newTask: TaskSeed?
    /// An item Home picks when it shows next, e.g. from a Bot's page.
    var showItem: String?
    var sidebar = NavigationSplitViewVisibility.all

    /// The open Task's terminal (its agent), whichever pane has the keyboard.
    /// Picking one of its shells makes that pane active; picking another
    /// terminal opens it.
    var terminal: String? {
        get { panes.first }
        set {
            guard let id = newValue else { return }
            if let at = panes.firstIndex(of: id) {
                active = at
            } else {
                panes = [id]
                active = 0
            }
            focusRequest += 1
        }
    }

    /// Moves the keyboard back to the active pane, as after a popup closes.
    func refocus() { focusRequest += 1 }

    /// The terminal of the pane that has the keyboard.
    var activePane: String? { panes.indices.contains(active) ? panes[active] : nil }

    func open(_ t: MuxaV2.Terminal) {
        terminal = t.id
        focus = true
    }

    /// Closes the Start Task sheet; a started Task opens in Focus.
    func taskStarted(_ t: MuxaV2.Terminal?) {
        newTask = nil
        if let t { open(t) }
    }

    /// The pane to the right of the active one, or the first after the last.
    func nextPane() { stepPane(by: 1) }

    func previousPane() { stepPane(by: -1) }

    private func stepPane(by step: Int) {
        guard panes.count > 1 else { return }
        active = (active + step + panes.count) % panes.count
        focusRequest += 1
    }

    /// `⌘1`…`⌘9`: `n` counts from 1.
    func focusPane(_ n: Int) {
        guard panes.indices.contains(n - 1) else { return }
        active = n - 1
        focusRequest += 1
    }

    /// The user clicked into a pane.
    func activate(_ index: Int) {
        if panes.indices.contains(index) { active = index }
    }

    /// The open Task's live shells, in the server's order. The pane with the
    /// keyboard keeps it while it is there; a new shell `focus` names takes it.
    func sync(shells: [String], focus: String? = nil) {
        guard let task = panes.first else { return }
        let current = activePane
        let next = [task] + shells
        guard next != panes || focus != nil else { return }
        panes = next
        active = (focus ?? current).flatMap(next.firstIndex(of:)) ?? 0
        focusRequest += 1
    }

    /// The open Task ended: Focus shows none.
    func closeTask() {
        panes = []
        active = 0
    }

    func toggleSidebar() {
        sidebar = sidebar == .detailOnly ? .all : .detailOnly
    }
}
