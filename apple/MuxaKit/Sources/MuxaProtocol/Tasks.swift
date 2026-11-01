import Foundation

/// A Task: a durable record of work, kept by the bridge on the host. It outlives
/// the tmux session that runs it (ADR 0015).
public struct TaskRecord: Codable, Equatable, Identifiable, Sendable {
    /// e.g. `t-7f3a2c`.
    public var id: String
    /// Grows by one on every write; `update-task` sends the one the client saw.
    public var rev: UInt64
    public var workspace: String
    public var title: String
    public var goal: String
    public var status: TaskStatus
    /// Epoch seconds.
    public var createdAt: UInt64
    /// Epoch seconds.
    public var updatedAt: UInt64
    public var project: String?
    /// The repo's main checkout, absolute.
    public var repo: String?
    public var branch: String?
    /// Where the agent works: the worktree or the task directory, absolute.
    public var dir: String?
    /// The last known session name, for display. Sessions link by `taskId`.
    public var session: String?
    /// From the bridge, e.g. why the launch failed.
    public var note: String?

    public init(
        id: String, rev: UInt64, workspace: String, title: String, goal: String, status: TaskStatus,
        createdAt: UInt64, updatedAt: UInt64, project: String? = nil, repo: String? = nil,
        branch: String? = nil, dir: String? = nil, session: String? = nil, note: String? = nil
    ) {
        self.id = id
        self.rev = rev
        self.workspace = workspace
        self.title = title
        self.goal = goal
        self.status = status
        self.createdAt = createdAt
        self.updatedAt = updatedAt
        self.project = project
        self.repo = repo
        self.branch = branch
        self.dir = dir
        self.session = session
        self.note = note
    }
}

/// Set by the user; separate from the agent's state. A status added by a newer
/// bridge decodes as `other`, so the record still reads.
public enum TaskStatus: Equatable, Hashable, Sendable {
    case open, review, done, dropped
    case other(String)

    public init(raw: String) {
        switch raw {
        case "open": self = .open
        case "review": self = .review
        case "done": self = .done
        case "dropped": self = .dropped
        default: self = .other(raw)
        }
    }

    public var raw: String {
        switch self {
        case .open: "open"
        case .review: "review"
        case .done: "done"
        case .dropped: "dropped"
        case .other(let raw): raw
        }
    }
}

extension TaskStatus: Codable {
    public init(from decoder: any Decoder) throws {
        self.init(raw: try decoder.singleValueContainer().decode(String.self))
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.singleValueContainer()
        try c.encode(raw)
    }
}

/// A workspace from the host's config; `create-task` names one.
public struct WorkspaceInfo: Codable, Equatable, Hashable, Identifiable, Sendable {
    public var name: String

    public var id: String { name }

    public init(name: String) {
        self.name = name
    }
}

/// The body of a `task-list` message.
public struct TaskListing: Codable, Equatable, Sendable {
    /// The id of the `list-tasks` request.
    public var tasks: UInt64
    public var workspaces: [WorkspaceInfo]
    public var entries: [TaskRecord]
    /// More records exist than were listed.
    public var truncated: Bool

    public init(tasks: UInt64, workspaces: [WorkspaceInfo], entries: [TaskRecord], truncated: Bool = false) {
        self.tasks = tasks
        self.workspaces = workspaces
        self.entries = entries
        self.truncated = truncated
    }

    private enum Key: String, CodingKey { case tasks, workspaces, entries, truncated }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: Key.self)
        tasks = try c.decode(UInt64.self, forKey: .tasks)
        workspaces = try c.decode([WorkspaceInfo].self, forKey: .workspaces)
        entries = try c.decode([TaskRecord].self, forKey: .entries)
        truncated = try c.decodeIfPresent(Bool.self, forKey: .truncated) ?? false
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.container(keyedBy: Key.self)
        try c.encode(tasks, forKey: .tasks)
        try c.encode(workspaces, forKey: .workspaces)
        try c.encode(entries, forKey: .entries)
        if truncated { try c.encode(true, forKey: .truncated) }
    }
}
