import Foundation
import MuxaProtocol

/// Where a task record stands, derived from its status and its session's agent
/// state (ADR 0015). Every state comes from an adapter; nothing is guessed.
public enum InboxGroup: Int, CaseIterable, Sendable, Identifiable {
    case needsAttention, readyForReview, running, noAgent, closed

    public var id: Self { self }

    public var title: String {
        switch self {
        case .needsAttention: "Needs attention"
        case .readyForReview: "Ready for review"
        case .running: "Running"
        case .noAgent: "No agent"
        case .closed: "Closed"
        }
    }
}

public enum InboxNeed: String, Sendable {
    case none, answer, agentError = "agent-error", loop, review, noAgent = "no-agent", noSession = "no-session"
}

/// A task record with the session that runs it, if any.
public struct TaskItem: Equatable, Sendable, Identifiable {
    public var record: TaskRecord
    /// The session whose `taskId` is the record's id.
    public var session: Session?
    /// The session's agent pane.
    public var agentPane: Pane?
    /// The adapter's state, while it can be believed.
    public var agent: Agent?
    public var group: InboxGroup

    public var need: InboxNeed {
        if group == .closed { return .none }
        if agent?.state == .waiting { return .answer }
        if agent?.state == .error { return .agentError }
        if record.loop?.needsUser == true { return .loop }
        if group == .running { return .none }
        if group == .readyForReview { return .review }
        return session == nil ? .noSession : .noAgent
    }

    public var id: String { record.id }

    public init(record: TaskRecord, sessions: [Session]) {
        self.record = record
        session = sessions.first { $0.taskId == record.id }
        agentPane = session.flatMap(Self.agentPane)
        if let pane = agentPane, let agent = pane.agent, agent.shown(shellForeground: pane.shellForeground) != nil {
            self.agent = agent
        }
        group = Self.group(status: record.status, loop: record.loop, agent: agent?.state)
    }

    /// The pane tagged as the agent, else the first visible pane an adapter reports on.
    private static func agentPane(_ session: Session) -> Pane? {
        session.panes.first { $0.role == .agent }
            ?? session.visiblePanes.first { $0.agent != nil }
    }

    private static func group(status: TaskStatus, loop: TaskLoop?, agent: AgentState?) -> InboxGroup {
        switch status {
        case .done, .dropped: return .closed
        case .open, .review, .other: break
        }
        if loop?.needsUser == true { return .needsAttention }
        switch agent {
        case nil: return .noAgent
        // An error is an adapter's state too, and the user has to look at it.
        case .waiting, .error: return .needsAttention
        case .running: return .running
        // Status review, or one a newer bridge added, fits nowhere else.
        case .idle, .done: return .readyForReview
        }
    }
}

/// One group of the Inbox, newest change first.
public struct InboxSection: Equatable, Sendable, Identifiable {
    public var group: InboxGroup
    public var items: [TaskItem]

    public var id: InboxGroup { group }
}

extension Array where Element == TaskRecord {
    /// The non-empty groups in their fixed order.
    public func inbox(sessions: [Session]) -> [InboxSection] {
        let items = map { TaskItem(record: $0, sessions: sessions) }
            .sorted { ($0.record.updatedAt, $0.record.id) > ($1.record.updatedAt, $1.record.id) }
        return InboxGroup.allCases.compactMap { group in
            let members = items.filter { $0.group == group }
            return members.isEmpty ? nil : InboxSection(group: group, items: members)
        }
    }
}
