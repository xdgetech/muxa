import Foundation

/// The shared capability of the newer features (ADR 0027): the host sends `work:<level>`.
public let workCapability = "work"
/// Level 1: Routines and stored Inbox entries (ADR 0026).
public let workRoutines: UInt32 = 1

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
}

/// The open run of a Routine. Its session carries `taskId` = `task`.
public struct RoutineRun: Codable, Equatable, Sendable {
    public var occurrence: String
    public var startedAt: UInt64
    public var task: String?
}

public struct RoutineSuccess: Codable, Equatable, Sendable {
    public var at: UInt64
    public var occurrence: String
    public var task: String
    public var published: UInt32
}

public struct RoutineFailure: Codable, Equatable, Sendable {
    public var at: UInt64
    public var occurrence: String
    public var task: String?
    public var reason: String
}

/// A stored Inbox entry: a Routine's briefing or proposal.
public struct InboxEntry: Codable, Equatable, Sendable {
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
}

public struct SourceLink: Codable, Equatable, Sendable {
    public var url: String
    public var title: String?
}

public struct Proposal: Codable, Equatable, Sendable {
    public var action: String
    public var target: String
    public var content: String
    /// `sha256:<hex>`.
    public var hash: String
}

public enum InboxAction: String, Codable, Sendable {
    case approve, dismiss, later
}
