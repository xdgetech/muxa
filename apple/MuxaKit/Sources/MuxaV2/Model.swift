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

    /// Hidden by Later at `now` (Unix seconds).
    public func isLater(now: Int64) -> Bool { (laterUntil ?? 0) > now }
}

/// Open items in the user's order, then recent done ones.
public struct Inbox: Codable, Equatable, Sendable {
    public var revision: UInt64
    public var items: [InboxItem]
    public var done: [InboxItem]

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

/// A place a client asks for (ADR 0068). A repo is absolute, or relative to
/// the Workspace's root.
public enum PlaceSpec: Equatable, Sendable {
    /// A new folder in the Workspace's tasks folder.
    case folder
    /// A worktree of `repo` on `branch`.
    case branch(repo: String, branch: String, base: String? = nil)
    /// The repo's main checkout, never written.
    case read(repo: String)

    var json: [String: String] {
        switch self {
        case .folder: return ["kind": "folder"]
        case let .branch(repo, branch, base):
            var m = ["kind": "branch", "repo": repo, "branch": branch]
            if let base { m["base"] = base }
            return m
        case let .read(repo): return ["kind": "read", "repo": repo]
        }
    }
}

/// A Task's place as it is, from its agent's `muxa.places` label; the
/// working place comes first.
public struct Place: Codable, Equatable, Sendable {
    public enum Kind: String, Codable, Sendable { case folder, branch, read }
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
    public var isTaskAgent: Bool { labels["muxa.kind"] == "task" && labels["muxa.role"] == "agent" }
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
