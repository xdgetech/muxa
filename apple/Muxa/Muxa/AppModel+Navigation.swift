import Foundation
import MuxaProtocol
import MuxaSession

/// What the sidebar shows, one at a time (ADR 0035 §1).
enum SidebarMode: String, CaseIterable, Identifiable {
    case inbox, tasks, sessions

    var id: Self { self }

    var title: String {
        switch self {
        case .inbox: "Inbox"
        case .tasks: "Tasks"
        case .sessions: "Sessions"
        }
    }
}

/// What the detail column shows: the sidebar's selection, or a session's pane.
enum Detail: Hashable {
    case item(String)
    /// A Work item or an unsorted Capture (ADR 0048).
    case work(String)
    case capture(String)
    case task(String)
    case routines
    case pane
}

/// Pushed in the detail column over its `Detail`.
enum DetailRoute: Hashable {
    case routine(workspace: String, name: String)
    /// On a phone, the pane opened from an item or a Task: Back returns there.
    case pane
}

/// The sidebar's mode and the detail column (ADR 0035). The sidebar never changes
/// its mode by itself; only the user's picker and keys do.
extension AppModel {
    private static let modeKey = "sidebarMode"

    /// The remembered mode. The UI tests pick one with `-mode <inbox|tasks|sessions>`;
    /// the demo starts in Inbox mode without it, so no test depends on the last one.
    static func launchMode() -> SidebarMode {
        let defaults = UserDefaults.standard
        #if DEBUG
        let arguments = ProcessInfo.processInfo.arguments
        if let i = arguments.firstIndex(of: "-mode"), i + 1 < arguments.count,
           let mode = SidebarMode(rawValue: arguments[i + 1]) {
            return mode
        }
        if arguments.contains("-demo") { return .inbox }
        #endif
        return defaults.string(forKey: modeKey).flatMap(SidebarMode.init(rawValue:)) ?? .inbox
    }

    /// The user picked `mode`. The detail keeps a pane, and otherwise shows what the
    /// mode would select: the first item in Inbox mode, nothing in Tasks mode.
    func setMode(_ new: SidebarMode) {
        mode = new
        UserDefaults.standard.set(new.rawValue, forKey: Self.modeKey)
        if showsPane { return }
        switch (new, detail) {
        case (.inbox, .item?), (.inbox, .work?), (.inbox, .capture?), (.tasks, .task?), (.tasks, .routines?): return
        case (.inbox, _): detail = firstItemBesideTheList
        case (.tasks, _): detail = nil
        case (.sessions, _): detail = session.watchedPane == nil ? nil : .pane
        }
        detailPath = []
    }

    /// A tablet shows the first item beside the list. A phone shows only the list, so
    /// it selects nothing until the user opens an item.
    private var firstItemBesideTheList: Detail? {
        phoneLayout ? nil : session.openItems.first.map { .item($0.id) }
    }

    /// Shows the sidebar's mode `new`, and the sidebar with it.
    func showMode(_ new: SidebarMode) {
        setMode(new)
        showSidebar()
    }

    func showSidebar() { sidebarRequest += 1 }
    func toggleSidebar() { sidebarToggle += 1 }

    /// A row of the sidebar: shown in the detail, pushed on a phone.
    func select(_ new: Detail) {
        detail = new
        detailPath = []
        detailRequest += 1
    }

    /// Shows the pane that `watch` made current. From an item or a Task, a phone
    /// pushes it over that screen; otherwise the pane is the detail.
    func presentPane(overSelection: Bool = false) {
        if !showsPane { paneViewPending = true }
        if overSelection, phoneLayout, let detail, detail != .pane {
            if detailPath.last != .pane { detailPath.append(.pane) }
        } else {
            detail = .pane
            detailPath = []
        }
        detailRequest += 1
        paneRequest += 1
    }

    /// A pane view that `presentPane` asked for appeared. The watch may have sent its
    /// screen before the view could take it, so it asks once more.
    func paneAppeared() async {
        guard paneViewPending else { return }
        paneViewPending = false
        await session.resync()
    }

    /// The pane is on screen, as the detail or pushed over it.
    var showsPane: Bool { detail == .pane || detailPath.last == .pane }

    /// A Task row (ADR 0044): its session, as a Sessions row opens it; its detail when
    /// it has no session or is closed.
    func openTaskRow(_ item: TaskItem) {
        guard let s = item.session, item.group != .closed else { return select(.task(item.id)) }
        Task { await open(s) }
    }

    func clearDetail() {
        detail = nil
        detailPath = []
    }

    /// At connect, Inbox and Tasks mode stay on their list; Sessions mode opens the
    /// default pane as before (ADR 0035 §6). Back from the background, what was shown
    /// stays: the session watches its pane again by itself.
    func connected(resuming: Bool = false) async {
        if resuming, detail != nil { return }
        switch mode {
        case .sessions:
            await openDefaultPane()
        case .inbox:
            if case .item(let id)? = detail, item(id) != nil { return }
            if case .work? = detail { return }
            if detail != .pane { detail = firstItemBesideTheList }
        case .tasks:
            break
        }
    }

    // MARK: The selection in the sidebar

    /// The item shown, as the detail or under a pane pushed over it.
    var shownItem: InboxItem? {
        guard case .item(let id)? = detail else { return nil }
        return item(id)
    }

    /// The row that owns what the detail shows is highlighted, in any mode.
    func highlights(_ item: InboxItem) -> Bool {
        if detail == .item(item.id) { return true }
        return detail == .pane && currentSession != nil && itemSession(item)?.id == currentSession?.id
    }

    func highlights(_ task: TaskItem) -> Bool {
        if detail == .task(task.id) { return true }
        return detail == .pane && currentSession != nil && task.session?.id == currentSession?.id
    }

    func highlights(_ s: Session) -> Bool { detail == .pane && s.id == currentSession?.id }

    /// After an answer: the next item on a tablet; a phone returns to the list.
    func itemAnswered(_ saved: InboxItem, before: [InboxItem]) {
        guard detail == .item(saved.id) else { return }
        let remaining = session.openItems.map(\.id).filter { $0 != saved.id }
        detail = InboxSelection.next(after: saved.id, in: before.map(\.id), remaining: remaining).map { .item($0) }
        detailPath = []
        if phoneLayout { showSidebar() }
    }

    /// Later (ADR 0040): the item moves to the bottom at once, and the next one opens
    /// where it was shown.
    func later(_ item: InboxItem) { triage(item, .later) }

    /// ✓ from a swipe: the item closes; its Task stays as it is.
    func done(_ item: InboxItem) {
        if let done = ItemAnswers(item).done { triage(item, done) }
    }

    private func triage(_ item: InboxItem, _ action: InboxAction) {
        let before = session.openItems
        Task {
            do {
                itemAnswered(try await session.answer(item, action), before: before)
            } catch {
                laterFailure = PaneSession.describe(error)
            }
        }
    }

    /// Ask Agent (ADR 0041): the item and `text` go to the workspace's Ask session,
    /// and its Chat opens over the item. Throws what the host said went wrong.
    func askAgent(_ item: InboxItem, text: String) async throws {
        let started = try await session.askAgent(item, text: text)
        guard let s = session.sessions.first(where: { $0.id == started.id }) else { return }
        await open(s, as: .chat)
    }

    /// An item left the list while it was shown, answered elsewhere: a tablet shows the next.
    func itemsChanged(from old: [String], to new: [String]) {
        guard !phoneLayout, case .item(let id)? = detail, !new.contains(id) else { return }
        detail = InboxSelection.next(after: id, in: old, remaining: new).map { .item($0) }
    }

    /// Cmd+Up/Down: the previous or next item, shown where the item was.
    func stepItem(_ offset: Int) {
        let items = session.openItems
        guard !items.isEmpty else { return }
        let index = items.firstIndex { detail == .item($0.id) } ?? 0
        detail = .item(items[min(max(index + offset, 0), items.count - 1)].id)
        detailPath = []
    }

    /// The item's agent has a Chat (ADR 0009).
    func hasChat(_ item: InboxItem) -> Bool {
        guard session.canChat, let s = itemSession(item) else { return false }
        let agent = s.panes.first { $0.role == .agent } ?? s.panes.first { $0.isAgentSide }
        return agent.map { $0.isAgentSide && !session.unsupportedChats.contains($0.id) } ?? false
    }
}

/// Which item takes an answered one's place.
enum InboxSelection {
    static func next(after id: String, in old: [String], remaining: [String]) -> String? {
        guard !remaining.isEmpty else { return nil }
        let index = old.firstIndex(of: id) ?? 0
        let later = old.dropFirst(index + 1).first { remaining.contains($0) }
        return later ?? remaining.last
    }
}
