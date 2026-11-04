import Foundation
import MuxaProtocol

/// A Routine run whose agent waits on the user. The run is not a Task, but its
/// need shows in Inbox (ADR 0026 section 3).
public struct RoutineNeed: Equatable, Sendable {
    public var routine: RoutineInfo
    /// The run's record id: the session's `taskId`.
    public var task: String
    public var session: Session
}

/// One row of the Inbox list: a Task need, a stored entry, or a waiting Routine run.
public enum InboxRow: Equatable, Sendable, Identifiable {
    case task(TaskItem)
    case entry(InboxEntry)
    case routine(RoutineNeed)

    public var id: String {
        switch self {
        case .task(let item): item.id
        case .entry(let entry): entry.id
        case .routine(let need): need.task
        }
    }
}

extension InboxEntry {
    public var isBriefing: Bool { kind == "briefing" }
    public var isProposal: Bool { kind == "proposal" && proposal != nil }

    /// Still waits for the user: open, or back from Later. An unknown disposition
    /// shows without actions.
    public func isShown(now: UInt64) -> Bool {
        switch disposition {
        case "open": true
        case "later": (until ?? 0) <= now
        case "approved", "dismissed": false
        default: true
        }
    }

    /// The host knows the kind and the disposition, so actions can be offered.
    public var hasKnownActions: Bool {
        (isBriefing || isProposal) && (disposition == "open" || disposition == "later")
    }
}

extension Array where Element == RoutineInfo {
    /// Runs whose agent waits on the user, matched by `running.task` to a session's `taskId`.
    public func needs(sessions: [Session]) -> [RoutineNeed] {
        compactMap { routine in
            guard let task = routine.running?.task,
                  let session = sessions.first(where: { $0.taskId == task }),
                  session.panes.contains(where: { !$0.shellForeground && $0.agent?.state == .waiting })
            else { return nil }
            return RoutineNeed(routine: routine, task: task, session: session)
        }
    }
}

/// The Inbox as one ordered list (ADR 0025, "List first"): Task needs in the host's
/// queue order, waiting Routine runs, the stored entries newest first, and last
/// the Tasks moved down, so Move down puts a Task at the tail of the whole list.
/// `queue` is the Task queue (`triageQueue`), already without hidden Tasks.
public func inboxRows(
    queue: [TaskItem], entries: [InboxEntry], routines: [RoutineInfo], sessions: [Session],
    now: UInt64 = UInt64(Date().timeIntervalSince1970)
) -> [InboxRow] {
    let waiting = queue.filter { $0.place?.order == nil }.map(InboxRow.task)
    let moved = queue.filter { $0.place?.order != nil }.map(InboxRow.task)
    let runs = routines.needs(sessions: sessions).map(InboxRow.routine)
    let stored = entries.filter { $0.isShown(now: now) }.map(InboxRow.entry)
    return waiting + runs + stored + moved
}

/// The times Later offers. Each is in the future, as the host requires.
public enum LaterChoice: CaseIterable, Sendable {
    case inAnHour, tomorrowMorning

    public var title: String {
        switch self {
        case .inAnHour: "In 1 hour"
        case .tomorrowMorning: "Tomorrow 9:00"
        }
    }

    /// Epoch seconds. Tomorrow is the next day in `calendar`'s time zone.
    public func until(now: Date = Date(), calendar: Calendar = .current) -> UInt64 {
        switch self {
        case .inAnHour:
            return UInt64(now.timeIntervalSince1970) + 3600
        case .tomorrowMorning:
            let today = calendar.startOfDay(for: now)
            let tomorrow = calendar.date(byAdding: .day, value: 1, to: today) ?? today.addingTimeInterval(86_400)
            let morning = calendar.date(bySettingHour: 9, minute: 0, second: 0, of: tomorrow) ?? tomorrow
            return UInt64(morning.timeIntervalSince1970)
        }
    }
}

/// The host refuses a Later time in the past or more than 366 days ahead.
public func laterProblem(_ date: Date, now: Date = Date()) -> String? {
    if date <= now { return "Pick a time in the future." }
    if date.timeIntervalSince(now) > 366 * 86_400 { return "Pick a time within a year." }
    return nil
}
