/// A tmux session as the bridge reports it.
public struct Session: Codable, Equatable, Sendable, Identifiable {
    public var id: String
    public var name: String
    public var repo: String?
    /// The repo's main checkout, absolute.
    public var repoDir: String?
    public var branch: String?
    public var worktree: String?
    public var task: String?
    public var panes: [Pane]

    public init(
        id: String, name: String, repo: String? = nil, repoDir: String? = nil, branch: String? = nil,
        worktree: String? = nil, task: String? = nil, panes: [Pane]
    ) {
        self.id = id
        self.name = name
        self.repo = repo
        self.repoDir = repoDir
        self.branch = branch
        self.worktree = worktree
        self.task = task
        self.panes = panes
    }
}

public struct Pane: Codable, Equatable, Sendable, Identifiable {
    public var id: String
    public var window: String
    public var index: UInt32
    public var role: Role
    public var cols: UInt16
    public var rows: UInt16
    public var command: String
    public var path: String
    public var dead: Bool
    /// The foreground process is a shell; an agent state is then likely stale.
    public var shellForeground: Bool
    public var agent: Agent?

    public init(
        id: String, window: String, index: UInt32, role: Role, cols: UInt16, rows: UInt16,
        command: String, path: String, dead: Bool, shellForeground: Bool, agent: Agent? = nil
    ) {
        self.id = id
        self.window = window
        self.index = index
        self.role = role
        self.cols = cols
        self.rows = rows
        self.command = command
        self.path = path
        self.dead = dead
        self.shellForeground = shellForeground
        self.agent = agent
    }
}

public enum Role: String, Codable, Sendable {
    case agent, shell, hidden, unknown

    // A role added by a newer bridge reads as unknown.
    public init(from decoder: any Decoder) throws {
        let raw = try decoder.singleValueContainer().decode(String.self)
        self = Role(rawValue: raw) ?? .unknown
    }
}

public struct Agent: Codable, Equatable, Sendable {
    public var state: AgentState
    /// Epoch seconds of the last state change.
    public var since: UInt64?

    public init(state: AgentState, since: UInt64? = nil) {
        self.state = state
        self.since = since
    }
}

public enum AgentState: String, Codable, Sendable {
    case running, waiting, idle, done, error
}
