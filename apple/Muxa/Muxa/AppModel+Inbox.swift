import Foundation
import MuxaProtocol
import MuxaSession

/// The Inbox list's actions (ADR 0025 "List first", ADR 0027 §4). Every one is
/// written on the host; nothing here changes a Task's status unless asked to.
extension AppModel {
    func row(_ id: String) -> InboxRow? { inboxRows.first { $0.id == id } }

    func entry(_ id: String) -> InboxEntry? { session.inboxEntries.first { $0.id == id } }

    /// Move down: to the end of the list, with its status unchanged.
    func moveDown(_ row: InboxRow) async throws {
        guard case .task(let item) = row else { return }
        _ = try await session.place(item.record, InboxChange(down: true))
    }

    /// Later: out of the list until `until`, epoch seconds.
    func later(_ row: InboxRow, until: UInt64) async throws {
        switch row {
        case .task(let item): _ = try await session.place(item.record, InboxChange(until: until))
        case .entry(let entry): _ = try await session.later(entry, until: until)
        case .routine: break
        }
    }

    /// Done: a briefing is acknowledged, which dismisses it. A Task is marked Done.
    func done(_ row: InboxRow) async throws {
        switch row {
        case .task(let item): _ = try await session.updateTask(item.record, status: .done)
        case .entry(let entry): _ = try await session.dismiss(entry)
        case .routine: break
        }
    }

    func dismiss(_ entry: InboxEntry) async throws {
        _ = try await session.dismiss(entry)
    }

    /// Approves a proposal and returns the one Task the host created for it.
    func approve(_ entry: InboxEntry) async throws -> String? {
        try await session.approve(entry).task
    }

    /// The local Snooze of a host without `task-inbox`.
    func snooze(_ item: TaskItem) {
        inboxSnoozes[item.id] = item.attentionKey
    }

    func routine(workspace: String, name: String) -> RoutineInfo? {
        session.routines.first { $0.workspace == workspace && $0.name == name }
    }

    /// The session of a Routine's open run, by its `taskId`.
    func runSession(_ routine: RoutineInfo) -> Session? {
        guard let task = routine.running?.task else { return nil }
        return session.sessions.first { $0.taskId == task && $0.defaultPane != nil }
    }
}

extension InboxRow {
    var title: String {
        switch self {
        case .task(let item): item.record.title
        case .entry(let entry): entry.title
        case .routine(let need): need.routine.name
        }
    }

    /// What the row needs, and where it comes from.
    var subtitle: String {
        switch self {
        case .task(let item): ([item.need.title, item.record.workspace] + [item.record.project].compactMap { $0 }).joined(separator: " · ")
        case .entry(let entry): "\(entry.kindTitle) · \(entry.workspace)/\(entry.routine)"
        case .routine(let need): "Routine run waits for your answer · \(need.routine.workspace)"
        }
    }

    var icon: String {
        switch self {
        case .task: Icon.tasks
        case .entry(let entry): entry.isProposal ? Icon.proposal : Icon.briefing
        case .routine: Icon.routine
        }
    }
}

extension InboxEntry {
    var kindTitle: String {
        switch kind {
        case "briefing": "Briefing"
        case "proposal": "Proposal"
        default: kind
        }
    }
}

extension UInt64 {
    /// Epoch seconds as a short local date and time.
    var shownDate: String {
        Date(timeIntervalSince1970: TimeInterval(self)).formatted(date: .abbreviated, time: .shortened)
    }
}
