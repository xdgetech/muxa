import Foundation

/// The shared capability of the newer features (ADR 0027): the host sends `work:<level>`.
public let workCapability = "work"
/// Level 1: Routines and stored Inbox entries (ADR 0026).
public let workRoutines: UInt32 = 1
/// Level 2: repos relative to the workspace root, a branch from the title,
/// and `start-session` (ADR 0031).
public let workAsk: UInt32 = 2
/// Level 5: `close-task` also closes an ask session, which carries `ask` (ADR 0039).
public let workCloseAsk: UInt32 = 5

/// The highest `work` level in `capabilities`; 0 when the host has none.
public func workLevel(_ capabilities: [String]) -> UInt32 {
    capabilities.compactMap { cap -> UInt32? in
        guard cap.hasPrefix(workCapability + ":") else { return nil }
        return UInt32(cap.dropFirst(workCapability.count + 1))
    }.max() ?? 0
}

/// A Routine from the host's `workspaces.toml`, with its schedule state. `(workspace, name)` is its id.
public struct RoutineInfo: Codable, Equatable, Sendable {
    public var workspace: String
    public var name: String
    /// Nil: the Routine runs only on `run-routine`.
    public var trigger: RoutineTrigger?
    public var enabled: Bool
    public var worker: String?
    public var project: String?
    /// Epoch seconds.
    public var nextDue: UInt64?
    public var lastSuccess: RoutineSuccess?
    public var lastFailure: RoutineFailure?
    public var running: RoutineRun?

    public init(workspace: String, name: String, trigger: RoutineTrigger? = nil, enabled: Bool,
                worker: String? = nil, project: String? = nil, nextDue: UInt64? = nil,
                lastSuccess: RoutineSuccess? = nil, lastFailure: RoutineFailure? = nil, running: RoutineRun? = nil) {
        self.workspace = workspace
        self.name = name
        self.trigger = trigger
        self.enabled = enabled
        self.worker = worker
        self.project = project
        self.nextDue = nextDue
        self.lastSuccess = lastSuccess
        self.lastFailure = lastFailure
        self.running = running
    }
}

public struct RoutineTrigger: Codable, Equatable, Sendable {
    /// `HH:MM`, 24-hour, local to `timezone`.
    public var at: String
    /// An IANA name, e.g. `Europe/Berlin`.
    public var timezone: String
    /// The five cron fields (ADR 0036). Nil from an older host: every day.
    public var schedule: String?

    public init(at: String, timezone: String, schedule: String? = nil) {
        self.at = at
        self.timezone = timezone
        self.schedule = schedule
    }
}

/// A workspace whose `.muxa/routines.toml` could not be used. It lists no Routines.
public struct RoutineProblem: Codable, Equatable, Sendable {
    public var workspace: String
    /// Names the file and the Routine. A TOML parse error may quote the file.
    public var reason: String

    public init(workspace: String, reason: String) {
        self.workspace = workspace
        self.reason = reason
    }
}

/// The open run of a Routine. Its session carries `taskId` = `task`.
public struct RoutineRun: Codable, Equatable, Sendable {
    public var occurrence: String
    public var startedAt: UInt64
    public var task: String?

    public init(occurrence: String, startedAt: UInt64, task: String? = nil) {
        self.occurrence = occurrence
        self.startedAt = startedAt
        self.task = task
    }
}

public struct RoutineSuccess: Codable, Equatable, Sendable {
    public var at: UInt64
    public var occurrence: String
    public var task: String
    public var published: UInt32

    public init(at: UInt64, occurrence: String, task: String, published: UInt32) {
        self.at = at
        self.occurrence = occurrence
        self.task = task
        self.published = published
    }
}

public struct RoutineFailure: Codable, Equatable, Sendable {
    public var at: UInt64
    public var occurrence: String
    public var task: String?
    public var reason: String

    public init(at: UInt64, occurrence: String, task: String? = nil, reason: String) {
        self.at = at
        self.occurrence = occurrence
        self.task = task
        self.reason = reason
    }
}

/// A stored Inbox entry: a Routine's briefing or proposal.
public struct InboxEntry: Codable, Equatable, Sendable, Identifiable {
    public var id: String
    public var workspace: String
    public var routine: String
    public var key: String
    /// `briefing` or `proposal`; a newer host may send another kind.
    public var kind: String
    public var title: String
    public var body: String
    public var links: [SourceLink]?
    public var createdAt: UInt64
    /// The `routine-run` record that published it.
    public var run: String
    public var occurrence: String
    /// `open`, `later`, `approved` or `dismissed`.
    public var disposition: String
    /// With `later`: epoch seconds.
    public var until: UInt64?
    public var proposal: Proposal?
    /// After approval: the Task that performs it.
    public var task: String?

    public init(
        id: String, workspace: String, routine: String, key: String, kind: String, title: String, body: String,
        links: [SourceLink]? = nil, createdAt: UInt64, run: String, occurrence: String, disposition: String,
        until: UInt64? = nil, proposal: Proposal? = nil, task: String? = nil
    ) {
        self.id = id
        self.workspace = workspace
        self.routine = routine
        self.key = key
        self.kind = kind
        self.title = title
        self.body = body
        self.links = links
        self.createdAt = createdAt
        self.run = run
        self.occurrence = occurrence
        self.disposition = disposition
        self.until = until
        self.proposal = proposal
        self.task = task
    }
}

public struct SourceLink: Codable, Equatable, Sendable {
    public var url: String
    public var title: String?

    public init(url: String, title: String? = nil) {
        self.url = url
        self.title = title
    }
}

public struct Proposal: Codable, Equatable, Sendable {
    public var action: String
    public var target: String
    public var content: String
    /// `sha256:<hex>`.
    public var hash: String

    public init(action: String, target: String, content: String, hash: String) {
        self.action = action
        self.target = target
        self.content = content
        self.hash = hash
    }
}

/// An answer in `update-inbox`. Level 1 has `approve`, `dismiss` and `later`;
/// the rest are Inbox items' answers (level 3, ADR 0033).
public enum InboxAction: String, Codable, Sendable {
    case approve, dismiss, later
    case reject, choose, accept
    case keepOpen = "keep-open"
    case redo
    case changeCourse = "change-course"
    case retry, drop, read
    /// Sends the text as it is; on an `ask` it denies the menu first (ADR 0033 §4a).
    case reply
}
