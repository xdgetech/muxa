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
