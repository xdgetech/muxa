import MuxaV2
import Observation
import SwiftUI

/// Where the user is: Home or Focus, the panes Focus shows, and its panels.
/// Views and the Mac's menu commands share it.
@MainActor
@Observable
final class NextNavigation {
    /// The touch app's main parts: the iPhone's tabs, the iPad sidebar's segments.
    enum Tab: Hashable { case inbox, tasks, routines, hosts }

    /// A Task is open: Focus on the Mac, the Task screen on a phone, and the
    /// sidebar's Tasks on an iPad.
    var focus = false
    var tab = Tab.inbox
    /// The terminals Focus shows side by side; the Mac splits, iOS shows one.
    private(set) var panes: [String] = []
    /// The pane that takes picks from the list and the keyboard.
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
    var sidebar = NavigationSplitViewVisibility.all

    static let maxPanes = 4

    /// The active pane's terminal. Picking a shown one makes its pane active.
    var terminal: String? {
        get { panes.indices.contains(active) ? panes[active] : nil }
        set {
            guard let id = newValue else { return }
            if let at = panes.firstIndex(of: id) {
                active = at
            } else if panes.indices.contains(active) {
                panes[active] = id
            } else {
                panes = [id]
                active = 0
            }
            focusRequest += 1
        }
    }

    func open(_ t: MuxaV2.Terminal) {
        terminal = t.id
        focus = true
    }

    /// Closes the Start Task sheet; a started Task opens in Focus.
    func taskStarted(_ t: MuxaV2.Terminal?) {
        newTask = nil
        if let t { open(t) }
    }

    /// A new pane with the first of `candidates` not shown yet.
    func split(from candidates: [MuxaV2.Terminal]) {
        guard panes.count < Self.maxPanes, let next = candidates.first(where: { !panes.contains($0.id) }) else { return }
        panes.append(next.id)
        active = panes.count - 1
        focusRequest += 1
    }

    func closePane() {
        guard panes.indices.contains(active) else { return }
        panes.remove(at: active)
        active = min(active, max(panes.count - 1, 0))
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

    /// Terminals that ended leave their panes.
    func keep(only live: Set<String>) {
        let kept = panes.filter(live.contains)
        guard kept != panes else { return }
        let current = terminal
        panes = kept
        active = current.flatMap(kept.firstIndex(of:)) ?? max(kept.count - 1, 0)
    }

    func toggleSidebar() {
        sidebar = sidebar == .detailOnly ? .all : .detailOnly
    }
}
