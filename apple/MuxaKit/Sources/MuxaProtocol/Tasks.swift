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
    /// The agents that worked on the task, oldest first (ADR 0018). Recorded only
    /// while a bridge started or restarted them; empty when none was.
    public var runs: [AgentRun]
    /// The task's Task Loop; nil when it has none (ADR 0019).
    public var loop: TaskLoop?
    /// A start or restart prompt the host still waits to type (ADR 0019).
    public var pendingPrompt: PendingPrompt?
    public var worker: String?
    public var restartOperation: RestartOperation?
    /// The Work item the Task was started for (ADR 0048).
    public var work: String?

    public init(
        id: String, rev: UInt64, workspace: String, title: String, goal: String, status: TaskStatus,
        createdAt: UInt64, updatedAt: UInt64, project: String? = nil, repo: String? = nil,
        branch: String? = nil, dir: String? = nil, session: String? = nil, note: String? = nil,
        runs: [AgentRun] = [], loop: TaskLoop? = nil, pendingPrompt: PendingPrompt? = nil, worker: String? = nil, restartOperation: RestartOperation? = nil,
        work: String? = nil
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
        self.runs = runs
        self.loop = loop
        self.pendingPrompt = pendingPrompt
        self.worker = worker
        self.restartOperation = restartOperation
        self.work = work
    }

    private enum Key: String, CodingKey {
        case id, rev, workspace, title, goal, status, createdAt, updatedAt
        case project, repo, branch, dir, session, note, runs, loop, pendingPrompt, worker, restartOperation, work
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: Key.self)
        id = try c.decode(String.self, forKey: .id)
        rev = try c.decode(UInt64.self, forKey: .rev)
        workspace = try c.decode(String.self, forKey: .workspace)
        title = try c.decode(String.self, forKey: .title)
        goal = try c.decode(String.self, forKey: .goal)
        status = try c.decode(TaskStatus.self, forKey: .status)
        createdAt = try c.decode(UInt64.self, forKey: .createdAt)
        updatedAt = try c.decode(UInt64.self, forKey: .updatedAt)
        project = try c.decodeIfPresent(String.self, forKey: .project)
        repo = try c.decodeIfPresent(String.self, forKey: .repo)
        branch = try c.decodeIfPresent(String.self, forKey: .branch)
        dir = try c.decodeIfPresent(String.self, forKey: .dir)
        session = try c.decodeIfPresent(String.self, forKey: .session)
        note = try c.decodeIfPresent(String.self, forKey: .note)
        runs = try c.decodeIfPresent([AgentRun].self, forKey: .runs) ?? []
        loop = try c.decodeIfPresent(TaskLoop.self, forKey: .loop)
        pendingPrompt = try c.decodeIfPresent(PendingPrompt.self, forKey: .pendingPrompt)
        worker = try c.decodeIfPresent(String.self, forKey: .worker)
        restartOperation = try c.decodeIfPresent(RestartOperation.self, forKey: .restartOperation)
        work = try c.decodeIfPresent(String.self, forKey: .work)
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.container(keyedBy: Key.self)
        try c.encode(id, forKey: .id)
        try c.encode(rev, forKey: .rev)
        try c.encode(workspace, forKey: .workspace)
        try c.encode(title, forKey: .title)
        try c.encode(goal, forKey: .goal)
        try c.encode(status, forKey: .status)
        try c.encode(createdAt, forKey: .createdAt)
        try c.encode(updatedAt, forKey: .updatedAt)
        try c.encodeIfPresent(project, forKey: .project)
        try c.encodeIfPresent(repo, forKey: .repo)
        try c.encodeIfPresent(branch, forKey: .branch)
        try c.encodeIfPresent(dir, forKey: .dir)
        try c.encodeIfPresent(session, forKey: .session)
        try c.encodeIfPresent(note, forKey: .note)
        if !runs.isEmpty { try c.encode(runs, forKey: .runs) }
        try c.encodeIfPresent(loop, forKey: .loop)
        try c.encodeIfPresent(pendingPrompt, forKey: .pendingPrompt)
        try c.encodeIfPresent(worker, forKey: .worker)
        try c.encodeIfPresent(restartOperation, forKey: .restartOperation)
        try c.encodeIfPresent(work, forKey: .work)
    }
}

/// One agent that worked on a task, as the bridge started or restarted it (ADR 0018).
public struct AgentRun: Codable, Equatable, Identifiable, Sendable {
    /// From 1, one higher than the previous run's.
    public var id: UInt32
    /// The adapter that saw the agent, e.g. `claude-code`; nil until one did.
    public var agent: String?
    /// Epoch seconds.
    public var startedAt: UInt64
    /// Epoch seconds when the bridge restarted it; nil otherwise.
    public var endedAt: UInt64?
    /// The agent's own session id.
    public var agentSession: String?
    /// The session's transcript on the host, absolute. A path only.
    public var transcript: String?
    public var reason: RunReason
    /// The worker requested for this run, or `default`. Nil on a legacy run: unrecorded.
    public var worker: String?
    public var outcome: String?

    public init(
        id: UInt32, agent: String? = nil, startedAt: UInt64, endedAt: UInt64? = nil,
        agentSession: String? = nil, transcript: String? = nil, reason: RunReason, worker: String? = nil, outcome: String? = nil
    ) {
        self.id = id
        self.agent = agent
        self.startedAt = startedAt
        self.endedAt = endedAt
        self.agentSession = agentSession
        self.transcript = transcript
        self.reason = reason
        self.worker = worker
        self.outcome = outcome
    }
}

/// Why the bridge started a run. A reason added by a newer bridge decodes as `other`.
public enum RunReason: Equatable, Hashable, Sendable {
    /// `create-task` started the agent.
    case start
    /// `restart-agent` replaced the agent.
    case restart
    case other(String)

    public init(raw: String) {
        switch raw {
        case "start": self = .start
        case "restart": self = .restart
        default: self = .other(raw)
        }
    }

    public var raw: String {
        switch self {
        case .start: "start"
        case .restart: "restart"
        case .other(let raw): raw
        }
    }
}

extension RunReason: Codable {
    public init(from decoder: any Decoder) throws {
        self.init(raw: try decoder.singleValueContainer().decode(String.self))
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.singleValueContainer()
        try c.encode(raw)
    }
}

/// Set by the user; separate from the agent's state. A passing Task Loop moves
/// `open` to `review` (ADR 0019). A status added by a newer bridge decodes as
/// `other`, so the record still reads.
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
    /// The root folder, absolute (ADR 0031); nil from older hosts. A repo
    /// under it shows by its relative path.
    public var root: String?
    /// The check commands a Task Loop may run here; nil means no loops.
    public var checks: WorkspaceChecks?
    /// Its main checkouts, absolute and sorted: the repos under its root and
    /// those the config lists.
    public var repos: [String]
    /// What a Task's end does with its "For the workspace" (`work` level 14, ADR 0053); nil
    /// from older hosts.
    public var memory: MemoryMode?
    /// Epoch seconds of the last change to `.muxa/WORKSPACE.md`; nil when it does not exist.
    public var memoryAt: UInt64?

    public var id: String { name }

    public init(
        name: String, root: String? = nil, checks: WorkspaceChecks? = nil, repos: [String] = [],
        memory: MemoryMode? = nil, memoryAt: UInt64? = nil
    ) {
        self.name = name
        self.root = root
        self.checks = checks
        self.repos = repos
        self.memory = memory
        self.memoryAt = memoryAt
    }

    private enum Key: String, CodingKey { case name, root, checks, repos, memory, memoryAt }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: Key.self)
        name = try c.decode(String.self, forKey: .name)
        root = try c.decodeIfPresent(String.self, forKey: .root)
        checks = try c.decodeIfPresent(WorkspaceChecks.self, forKey: .checks)
        repos = try c.decodeIfPresent([String].self, forKey: .repos) ?? []
        memory = try c.decodeIfPresent(MemoryMode.self, forKey: .memory)
        memoryAt = try c.decodeIfPresent(UInt64.self, forKey: .memoryAt)
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.container(keyedBy: Key.self)
        try c.encode(name, forKey: .name)
        try c.encodeIfPresent(root, forKey: .root)
        try c.encodeIfPresent(checks, forKey: .checks)
        if !repos.isEmpty { try c.encode(repos, forKey: .repos) }
        try c.encodeIfPresent(memory, forKey: .memory)
        try c.encodeIfPresent(memoryAt, forKey: .memoryAt)
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
    public var workers: [WorkerInfo]

    public init(tasks: UInt64, workspaces: [WorkspaceInfo], entries: [TaskRecord], truncated: Bool = false, workers: [WorkerInfo] = []) {
        self.tasks = tasks
        self.workspaces = workspaces
        self.entries = entries
        self.truncated = truncated
        self.workers = workers
    }

    private enum Key: String, CodingKey { case tasks, workspaces, entries, truncated, workers }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: Key.self)
        tasks = try c.decode(UInt64.self, forKey: .tasks)
        workspaces = try c.decode([WorkspaceInfo].self, forKey: .workspaces)
        entries = try c.decode([TaskRecord].self, forKey: .entries)
        truncated = try c.decodeIfPresent(Bool.self, forKey: .truncated) ?? false
        workers = try c.decodeIfPresent([WorkerInfo].self, forKey: .workers) ?? []
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.container(keyedBy: Key.self)
        try c.encode(tasks, forKey: .tasks)
        try c.encode(workspaces, forKey: .workspaces)
        try c.encode(entries, forKey: .entries)
        if truncated { try c.encode(true, forKey: .truncated) }
        if !workers.isEmpty { try c.encode(workers, forKey: .workers) }
    }
}
