import Foundation

// Task proposals (ADR 0060), at `work` level 18: a staff member's Result or the secretary's
// `propose-task` carries a Task to start, and Start Task in the Inbox starts it in plan mode.

/// A Task the assistant proposes. Start Task starts it once, through `create-task`, on the
/// workspace's default Task worker. Never logged.
public struct TaskProposal: Codable, Equatable, Sendable {
    public var workspace: String
    /// A repo of the workspace, as `create-task`'s `repo`. Nil: the Task gets its own folder.
    public var repo: String?
    /// One line, at most 200 characters.
    public var title: String
    /// What the Task's agent does; at most 4096 bytes.
    public var goal: String
    /// The Work item the Task is for (`w-…`).
    public var work: String?
    /// On a `result` item: the step whose Result proposed it.
    public var step: UInt32?
    /// On a `result` item: the staff member who proposed it.
    public var worker: String?

    public init(
        workspace: String, repo: String? = nil, title: String, goal: String, work: String? = nil,
        step: UInt32? = nil, worker: String? = nil
    ) {
        self.workspace = workspace
        self.repo = repo
        self.title = title
        self.goal = goal
        self.work = work
        self.step = step
        self.worker = worker
    }
}

/// The overall workspace's runs today (ADR 0059 §5), for "N of M runs left today".
public struct RunBudget: Codable, Equatable, Sendable {
    /// Runs a plan may still start today.
    public var left: UInt32
    /// The overall workspace's `max_runs_per_day`.
    public var max: UInt32

    public init(left: UInt32, max: UInt32) {
        self.left = left
        self.max = max
    }
}
