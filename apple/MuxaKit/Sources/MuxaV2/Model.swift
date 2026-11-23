import Foundation

/// What an Inbox item asks of the user (ADR 0066).
public enum ItemKind: String, Codable, Sendable, CaseIterable {
    case review, reply, decide, read, todo, failed
}

public struct InboxItem: Codable, Equatable, Sendable, Identifiable {
    public var id: String
    public var kind: ItemKind
    public var title: String
    public var note: String?
    public var workspace: String?
    /// `YYYY-MM-DD`.
    public var due: String?
    /// Hidden until then (Unix seconds).
    public var laterUntil: Int64?
    public var created: Int64
    /// `capture`, `task` or `routine`.
    public var source: String
    /// The Task agent terminal that made the item.
    public var terminal: String?
    /// The Routine that made the item.
    public var routine: RoutineRef?
    /// Where a Task of muxa v1 worked; Resume Task starts its agent there.
    public var place: ItemPlace? = nil
    /// Pages the item is about, e.g. a PR or a Slack thread (ADR 0082).
    public var links: [ItemLink]? = nil
    /// What a Decide item proposes; Start Task takes it as the goal.
    public var proposal: ItemProposal? = nil
    /// The Bot of `workspace` whose run made the item.
    public var bot: String? = nil

    /// Hidden by Later at `now` (Unix seconds).
    public func isLater(now: Int64) -> Bool { (laterUntil ?? 0) > now }
}

/// A page an item is about: `http` or `https`.
public struct ItemLink: Codable, Equatable, Sendable {
    public var url: String
    public var title: String?

    public init(url: String, title: String? = nil) {
        self.url = url
        self.title = title
    }
}

/// A change a Routine proposes. muxa never performs it by itself.
public struct ItemProposal: Codable, Equatable, Sendable {
    public var action: String
    public var target: String
    /// The exact text to write.
    public var content: String

    public init(action: String, target: String, content: String) {
        self.action = action
        self.target = target
        self.content = content
    }
}

/// How a Bot's items ended (ADR 0082).
public struct BotCount: Codable, Equatable, Sendable {
    public var workspace: String
    public var bot: String
    public var done: UInt32
    public var deleted: UInt32

    public init(workspace: String, bot: String, done: UInt32, deleted: UInt32) {
        self.workspace = workspace
        self.bot = bot
        self.done = done
        self.deleted = deleted
    }
}

/// A Routine's id.
public struct RoutineRef: Codable, Equatable, Sendable {
    public var workspace: String
    public var name: String

    public init(workspace: String, name: String) {
        self.workspace = workspace
        self.name = name
    }
}

/// An existing working place in a Workspace.
public struct ItemPlace: Codable, Equatable, Sendable {
    public var workspace: String
    public var path: String

    public init(workspace: String, path: String) {
        self.workspace = workspace
        self.path = path
    }
}

/// What closing a Task does with a branch worktree that holds work.
public enum WorktreeChoice: String, Codable, Sendable {
    /// Only a clean and merged worktree goes; else `refused(code: "would-lose-work")`.
    case remove
    /// It goes anyway, with its branch.
    case force
}

/// A Workspace as a client may see it, for starting a Task. Nothing secret.
public struct Workspace: Codable, Equatable, Sendable, Identifiable {
    public var name: String
    public var root: String
    /// The repos as `startTask` takes them.
    public var repos: [String]
    /// The agent's program name, e.g. `claude`.
    public var agent: String?
    /// Nil when the Workspace runs no Task Loops.
    public var loop: WorkspaceLoop?
    public var connections: [WorkspaceConnection]
    /// The MCP servers a Task can name (ADR 0069).
    public var mcp: [WorkspaceMcp]
    /// Each repo's checked-out branch, by its name in `repos`.
    public var branches: [String: String]

    public var id: String { name }

    public init(name: String, root: String, repos: [String] = [], agent: String? = nil, loop: WorkspaceLoop? = nil,
                connections: [WorkspaceConnection] = [], mcp: [WorkspaceMcp] = [], branches: [String: String] = [:]) {
        self.name = name
        self.root = root
        self.repos = repos
        self.agent = agent
        self.loop = loop
        self.connections = connections
        self.mcp = mcp
        self.branches = branches
    }

    enum CodingKeys: String, CodingKey {
        case name, root, repos, agent, loop, connections, mcp, branches
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        name = try c.decode(String.self, forKey: .name)
        root = try c.decode(String.self, forKey: .root)
        repos = try c.decode([String].self, forKey: .repos)
        agent = try c.decodeIfPresent(String.self, forKey: .agent)
        loop = try c.decodeIfPresent(WorkspaceLoop.self, forKey: .loop)
        connections = try c.decode([WorkspaceConnection].self, forKey: .connections)
        // Servers before E2b send no list.
        mcp = try c.decodeIfPresent([WorkspaceMcp].self, forKey: .mcp) ?? []
        branches = try c.decodeIfPresent([String: String].self, forKey: .branches) ?? [:]
    }
}

/// An MCP server's name only; its command and variables stay on the host.
public struct WorkspaceMcp: Codable, Equatable, Sendable, Identifiable {
    public var name: String
    /// Always true since ADR 0075: every agent of the Workspace gets it.
    public var isDefault: Bool
    /// Who gets it now (ADR 0069 §7).
    public var usedBy: UsedBy
    /// Unix seconds when an agent last started with it; nil: never.
    public var lastUsed: Int64?

    public var id: String { name }

    public init(name: String, isDefault: Bool, usedBy: UsedBy = UsedBy(), lastUsed: Int64? = nil) {
        self.name = name
        self.isDefault = isDefault
        self.usedBy = usedBy
        self.lastUsed = lastUsed
    }

    enum CodingKeys: String, CodingKey {
        case name
        case isDefault = "default"
        case usedBy
        case lastUsed
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        name = try c.decode(String.self, forKey: .name)
        isDefault = try c.decode(Bool.self, forKey: .isDefault)
        // Servers before ADR 0069 §7 send neither.
        usedBy = try c.decodeIfPresent(UsedBy.self, forKey: .usedBy) ?? UsedBy()
        lastUsed = try c.decodeIfPresent(Int64.self, forKey: .lastUsed)
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encode(name, forKey: .name)
        try c.encode(isDefault, forKey: .isDefault)
        try c.encode(usedBy, forKey: .usedBy)
        try c.encode(lastUsed, forKey: .lastUsed)
    }
}

/// Who gets a connection or MCP server now: a default one counts for every
/// Task and Routine of the Workspace.
public struct UsedBy: Codable, Equatable, Sendable {
    /// The names of the Routines in `routines.toml` that get it.
    public var routines: [String]
    /// The ids of the live Task agent terminals that got it.
    public var tasks: [String]

    public init(routines: [String] = [], tasks: [String] = []) {
        self.routines = routines
        self.tasks = tasks
    }

    enum CodingKeys: String, CodingKey { case routines, tasks }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        routines = try c.decodeIfPresent([String].self, forKey: .routines) ?? []
        tasks = try c.decodeIfPresent([String].self, forKey: .tasks) ?? []
    }
}

public struct WorkspaceLoop: Codable, Equatable, Sendable {
    public var checks: LoopChecks

    public init(checks: LoopChecks) { self.checks = checks }
}

/// The checks a Task Loop may run: `"any"` or a list on the wire.
public enum LoopChecks: Codable, Equatable, Sendable {
    case any
    case list([String])

    public init(from decoder: any Decoder) throws {
        let c = try decoder.singleValueContainer()
        if let list = try? c.decode([String].self) {
            self = .list(list)
            return
        }
        let word = try c.decode(String.self)
        guard word == "any" else {
            throw DecodingError.dataCorruptedError(in: c, debugDescription: "checks is \"any\" or a list, not \(word)")
        }
        self = .any
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.singleValueContainer()
        switch self {
        case .any: try c.encode("any")
        case let .list(list): try c.encode(list)
        }
    }
}

/// A connection's name only; its secret never crosses the wire (ADR 0069).
public struct WorkspaceConnection: Codable, Equatable, Sendable, Identifiable {
    public var name: String
    /// Always true since ADR 0075: every agent of the Workspace gets it.
    public var isDefault: Bool
    /// Who gets it now (ADR 0069 §7).
    public var usedBy: UsedBy
    /// Unix seconds when an agent last started with it; nil: never.
    public var lastUsed: Int64?

    public var id: String { name }

    public init(name: String, isDefault: Bool, usedBy: UsedBy = UsedBy(), lastUsed: Int64? = nil) {
        self.name = name
        self.isDefault = isDefault
        self.usedBy = usedBy
        self.lastUsed = lastUsed
    }

    enum CodingKeys: String, CodingKey {
        case name
        case isDefault = "default"
        case usedBy
        case lastUsed
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        name = try c.decode(String.self, forKey: .name)
        isDefault = try c.decode(Bool.self, forKey: .isDefault)
        // Servers before ADR 0069 §7 send neither.
        usedBy = try c.decodeIfPresent(UsedBy.self, forKey: .usedBy) ?? UsedBy()
        lastUsed = try c.decodeIfPresent(Int64.self, forKey: .lastUsed)
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encode(name, forKey: .name)
        try c.encode(isDefault, forKey: .isDefault)
        try c.encode(usedBy, forKey: .usedBy)
        try c.encode(lastUsed, forKey: .lastUsed)
    }
}

/// Open items in the user's order, then recent done ones.
public struct Inbox: Codable, Equatable, Sendable {
    public var revision: UInt64
    public var items: [InboxItem]
    public var done: [InboxItem]
    /// Per Bot, its items that ended Done and those deleted while open.
    public var bots: [BotCount]? = nil

    public static let empty = Inbox(revision: 0, items: [], done: [])
}

/// A Capture line read into fields, shown as chips before the item is added.
public struct Capture: Codable, Equatable, Sendable {
    public var kind: ItemKind
    public var title: String
    public var workspace: String?
    public var due: String?

    public init(kind: ItemKind, title: String, workspace: String? = nil, due: String? = nil) {
        self.kind = kind
        self.title = title
        self.workspace = workspace
        self.due = due
    }
}

/// A Routine as the host defines it, with its last run. `workspace/name` is its id.
public struct Routine: Codable, Equatable, Sendable, Identifiable {
    public var workspace: String
    public var name: String
    public var enabled: Bool
    /// `HH:MM` in `timezone`; nil for a Routine that runs only on request.
    public var at: String?
    public var timezone: String?
    /// The five cron fields; nil means every day.
    public var schedule: String?
    public var nextDue: Int64?
    public var runningSince: Int64?
    public var last: RoutineLast?

    public var id: String { "\(workspace)/\(name)" }
}

public struct RoutineLast: Codable, Equatable, Sendable {
    public var at: Int64
    public var ok: Bool
    public var reason: String?
    public var published: UInt32?
}

public struct RoutineProblem: Codable, Equatable, Sendable {
    public var workspace: String
    public var reason: String
}

/// A Workspace whose Routines are in its file but turned off: none run.
public struct RoutinesOff: Codable, Equatable, Sendable {
    public var workspace: String
    public var count: UInt32

    public init(workspace: String, count: UInt32) {
        self.workspace = workspace
        self.count = count
    }
}

/// A place a client asks for (ADR 0068). A repo is absolute, or relative to
/// the Workspace's root.
public enum PlaceSpec: Equatable, Sendable {
    /// A new folder in the Workspace's tasks folder.
    case folder
    /// A worktree of `repo` on `branch`.
    case branch(repo: String, branch: String, base: String? = nil)
    /// The repo's main checkout, never written.
    case read(repo: String)
    /// A folder or worktree that already exists: nothing is made. It is the
    /// working place, so it never comes with a folder or a branch.
    case existing(path: String)
    /// The repo's own checkout as the working place (ADR 0077); alone.
    case checkout(repo: String)
    /// Every repo of the Workspace read-only, beside the Task's folder.
    case allRead

    var json: [String: String] {
        switch self {
        case .folder: return ["kind": "folder"]
        case let .branch(repo, branch, base):
            var m = ["kind": "branch", "repo": repo, "branch": branch]
            if let base { m["base"] = base }
            return m
        case let .read(repo): return ["kind": "read", "repo": repo]
        case let .existing(path): return ["kind": "existing", "path": path]
        case let .checkout(repo): return ["kind": "checkout", "repo": repo]
        case .allRead: return ["kind": "all-read"]
        }
    }
}

/// A Task's place as it is, from its agent's `muxa.places` label; the
/// working place comes first.
public struct Place: Codable, Equatable, Sendable {
    public enum Kind: String, Codable, Sendable { case folder, branch, read, checkout }
    public var kind: Kind
    public var path: String
    public var branch: String?
}

public struct TerminalStatus: Codable, Equatable, Sendable {
    /// What the program reported with OSC 7501: `working`, `blocked`, `done`…
    public var state: String
    public var app: String?
}

/// A terminal's facts; never its screen.
public struct Terminal: Codable, Equatable, Sendable, Identifiable {
    public var id: String
    public var name: String?
    public var live: Bool
    public var status: TerminalStatus?
    public var labels: [String: String]
    public var cols: UInt16
    public var rows: UInt16

    /// The worktree of a Task's agent.
    public var taskDir: String? { labels["muxa.dir"] }
    public var branch: String? { labels["muxa.branch"] }
    /// A Task's title as the user wrote it; the name is only a slug of it.
    public var taskTitle: String? { labels["muxa.title"] }
    public var isTaskAgent: Bool { labels["muxa.kind"] == "task" && labels["muxa.role"] == "agent" }
    /// A shell beside a Task's agent (ADR 0079); it shares the agent's `taskDir`.
    public var isTaskShell: Bool { labels["muxa.kind"] == "task" && labels["muxa.role"] == "shell" }
    /// A Task's places, the working place first; empty for other terminals.
    public var places: [Place] {
        guard let text = labels["muxa.places"], let data = text.data(using: .utf8) else { return [] }
        return (try? JSONDecoder().decode([Place].self, from: data)) ?? []
    }
}

/// What the Reader lists and opens (plan F2).
public enum ReaderKind: String, Codable, Sendable, CaseIterable {
    case docs, changes, images
}

/// Which diff of a changed file.
public enum DiffScope: String, Codable, Sendable {
    /// Against HEAD: staged, unstaged and untracked.
    case uncommitted
    /// The branch's commits since it left its base.
    case branch
}

/// A file of a terminal's folder the Reader may open. Which fields are set
/// depends on the kind.
public struct ReaderEntry: Codable, Equatable, Sendable, Identifiable {
    public var path: String
    /// Documents: an HTML `<title>`.
    public var title: String?
    public var mtime: Int64?
    /// Images: bytes.
    public var size: Int64?
    /// Changes: `modified`, `added`, `deleted`, `renamed`, `untracked`, `conflicted`.
    public var status: String?
    public var oldPath: String?
    public var added: UInt32?
    public var removed: UInt32?

    public var id: String { path }

    public init(path: String, title: String? = nil, mtime: Int64? = nil, size: Int64? = nil, status: String? = nil,
                oldPath: String? = nil, added: UInt32? = nil, removed: UInt32? = nil) {
        self.path = path
        self.title = title
        self.mtime = mtime
        self.size = size
        self.status = status
        self.oldPath = oldPath
        self.added = added
        self.removed = removed
    }
}

public struct ReaderBranch: Codable, Equatable, Sendable {
    /// The ref compared with, e.g. `origin/main`.
    public var base: String
    public var commits: UInt32
    public var entries: [ReaderEntry]
    public var truncated: Bool?
}

public struct ReaderList: Codable, Equatable, Sendable {
    /// The repo's folder name; nil outside a repo.
    public var root: String?
    public var entries: [ReaderEntry]
    public var truncated: Bool?
    /// Changes: what the branch's commits changed.
    public var branch: ReaderBranch?

    public init(root: String? = nil, entries: [ReaderEntry], truncated: Bool? = nil, branch: ReaderBranch? = nil) {
        self.root = root
        self.entries = entries
        self.truncated = truncated
        self.branch = branch
    }
}

/// An open file as it is now.
public struct ReaderContent: Codable, Equatable, Sendable {
    public var path: String
    /// Documents and diffs.
    public var text: String?
    /// Images: the bytes in base64.
    public var data: String?
    /// A diff of a binary file.
    public var binary: Bool?
    public var mtime: Int64?

    public init(path: String, text: String? = nil, data: String? = nil, binary: Bool? = nil, mtime: Int64? = nil) {
        self.path = path
        self.text = text
        self.data = data
        self.binary = binary
        self.mtime = mtime
    }

    /// An image's bytes.
    public var bytes: Data? { data.flatMap { Data(base64Encoded: $0) } }
}

/// A Task Loop a client asks for (ADR 0019).
public struct LoopSpec: Codable, Equatable, Sendable {
    /// Nil: the Workspace's only allowed check.
    public var check: String?
    /// 1 to 10; nil means 3.
    public var maxAttempts: UInt32?
    /// Seconds one check may run; nil means 600.
    public var timeout: UInt64?

    public init(check: String? = nil, maxAttempts: UInt32? = nil, timeout: UInt64? = nil) {
        self.check = check
        self.maxAttempts = maxAttempts
        self.timeout = timeout
    }

    var json: [String: Any] {
        var m: [String: Any] = [:]
        if let check { m["check"] = check }
        if let maxAttempts { m["max_attempts"] = maxAttempts }
        if let timeout { m["timeout"] = timeout }
        return m
    }
}

/// Where a Task's loop stands. Times are Unix seconds.
public struct LoopInfo: Codable, Equatable, Sendable, Identifiable {
    public var terminal: String
    public var check: String
    public var maxAttempts: UInt32
    public var timeout: UInt64
    /// `armed`, `checking`, `paused`, `passed` or `stopped`.
    public var state: String
    public var attempts: UInt32
    public var armedAt: Int64
    public var since: Int64
    /// `waiting`, `limit`, `error`, `no-agent`, `not-sent`, `user`…
    public var reason: String?
    public var lastCheck: LastCheck?

    public var id: String { terminal }
    /// It still works on the Task.
    public var isActive: Bool { ["armed", "checking", "paused"].contains(state) }

    public init(terminal: String, check: String, maxAttempts: UInt32, timeout: UInt64, state: String, attempts: UInt32,
                armedAt: Int64, since: Int64, reason: String? = nil, lastCheck: LastCheck? = nil) {
        self.terminal = terminal
        self.check = check
        self.maxAttempts = maxAttempts
        self.timeout = timeout
        self.state = state
        self.attempts = attempts
        self.armedAt = armedAt
        self.since = since
        self.reason = reason
        self.lastCheck = lastCheck
    }
}

public struct LastCheck: Codable, Equatable, Sendable {
    public var attempt: UInt32
    /// Nil when the check timed out.
    public var exit: Int32?
    public var seconds: UInt64
    public var at: Int64
}

/// One item of an agent's conversation (plan F4).
public struct ChatEntry: Codable, Equatable, Sendable, Identifiable {
    public var id: String
    /// `user`, `assistant`, `tool` or `notice`.
    public var kind: String
    public var text: String?
    public var tool: ChatTool?

    public init(id: String, kind: String, text: String? = nil, tool: ChatTool? = nil) {
        self.id = id
        self.kind = kind
        self.text = text
        self.tool = tool
    }
}

public struct ChatTool: Codable, Equatable, Sendable {
    public var name: String
    /// One line about the call, e.g. the command or the file.
    public var summary: String
    public var result: String?
    public var error: Bool?

    public init(name: String, summary: String, result: String? = nil, error: Bool? = nil) {
        self.name = name
        self.summary = summary
        self.result = result
        self.error = error
    }
}

/// What the agent waits on, with the input that picks each option.
public struct ChatPrompt: Codable, Equatable, Sendable {
    public var entry: String
    /// `permission`, `question` or `plan`.
    public var kind: String
    public var question: String?
    public var options: [ChatOption]

    public init(entry: String, kind: String, question: String? = nil, options: [ChatOption]) {
        self.entry = entry
        self.kind = kind
        self.question = question
        self.options = options
    }
}

public struct ChatOption: Codable, Equatable, Sendable, Identifiable {
    public var label: String
    /// Sent as the terminal's input to pick this option.
    public var input: String
    public var cancel: Bool?

    public var id: String { label }

    public init(label: String, input: String, cancel: Bool? = nil) {
        self.label = label
        self.input = input
        self.cancel = cancel
    }
}

/// What changed in a followed conversation.
public struct ChatUpdate: Codable, Equatable, Sendable {
    /// `entries` replace everything; else each replaces the entry with its id, or is added.
    public var reset: Bool?
    public var entries: [ChatEntry]
    public var truncated: Bool?
    /// What the agent waits on now; nil: nothing known.
    public var prompt: ChatPrompt?

    public init(reset: Bool? = nil, entries: [ChatEntry], truncated: Bool? = nil, prompt: ChatPrompt? = nil) {
        self.reset = reset
        self.entries = entries
        self.truncated = truncated
        self.prompt = prompt
    }

    /// `entries` after this update.
    public func applied(to entries: [ChatEntry]) -> [ChatEntry] {
        if reset == true { return self.entries }
        var out = entries
        for e in self.entries {
            if let at = out.firstIndex(where: { $0.id == e.id }) { out[at] = e } else { out.append(e) }
        }
        return out
    }
}

public struct Exit: Codable, Equatable, Sendable {
    public var reason: String
    public var exitCode: Int64?
    public var signal: Int64?
}

/// What the server sends besides replies.
public enum V2Event: Equatable, Sendable {
    /// Output to show; bytes the client already had are cut off.
    case output(channel: UInt32, bytes: [UInt8])
    /// The next output starts a fresh screen.
    case paint(channel: UInt32)
    /// The channel is over: the exit, or nil when detached or not resumed.
    case closed(channel: UInt32, exit: Exit?)
    case inbox(Inbox)
    case changed(Terminal)
    case removed(terminal: String)
    /// Changes were lost: list the terminals again.
    case resync
    /// A followed conversation changed.
    case chat(chat: UInt32, update: ChatUpdate)
    /// A followed conversation ended; the chat is closed.
    case chatEnded(chat: UInt32, reason: String)
    /// A Task Loop changed; every loop as it is now.
    case loops([LoopInfo])
    /// An open Reader file changed.
    case reader(reader: UInt32, content: ReaderContent)
    /// An open Reader file ended; the reader is closed.
    case readerEnded(reader: UInt32, reason: String)
    /// The connection is gone; resume with `V2Client.state`.
    case lost(String)
}

public enum V2Error: Error, Equatable, Sendable {
    /// The server answered with an error: its code and message.
    case refused(code: String, message: String)
    case lost(String)
    case badMessage(String)
}

enum Coding {
    static let decoder: JSONDecoder = {
        let d = JSONDecoder()
        d.keyDecodingStrategy = .convertFromSnakeCase
        return d
    }()
}
