import Foundation

/// `"any"` or a list of exact commands, from the host's `[workspace.loop]`.
public enum WorkspaceChecks: Equatable, Hashable, Sendable {
    /// Any command, at most 4096 bytes, with no control character.
    case any
    /// Only these commands, exactly.
    case list([String])
}

extension WorkspaceChecks: Codable {
    public init(from decoder: any Decoder) throws {
        let c = try decoder.singleValueContainer()
        if let word = try? c.decode(String.self) {
            guard word == "any" else {
                throw DecodingError.dataCorruptedError(in: c, debugDescription: "checks must be \"any\" or a list")
            }
            self = .any
        } else {
            self = .list(try c.decode([String].self))
        }
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.singleValueContainer()
        switch self {
        case .any: try c.encode("any")
        case .list(let list): try c.encode(list)
        }
    }
}

/// What a client asks for when it starts a Task Loop (ADR 0019). The host fills
/// in the defaults.
public struct LoopSpec: Codable, Equatable, Sendable {
    /// A shell command, run in the task's `dir`; one of the workspace's `checks`.
    public var check: String
    /// Failed checks before the loop stops: 1 to 10, default 3.
    public var maxAttempts: UInt32?
    /// Seconds one check may run: 10 to 3600, default 600.
    public var timeout: UInt64?

    public init(check: String, maxAttempts: UInt32? = nil, timeout: UInt64? = nil) {
        self.check = check
        self.maxAttempts = maxAttempts
        self.timeout = timeout
    }
}

/// A task's Task Loop: the host runs `check` after each finished turn of the
/// agent, and types a fail prompt until it passes (ADR 0019).
public struct TaskLoop: Codable, Equatable, Sendable {
    public var check: String
    public var maxAttempts: UInt32
    /// Seconds.
    public var timeout: UInt64
    public var state: LoopState
    /// Failed checks so far.
    public var attempts: UInt32
    /// Epoch seconds: when the loop started or last typed a prompt.
    public var armedAt: UInt64
    /// Epoch seconds of the last state change.
    public var since: UInt64
    /// Set when the state is `paused` or `stopped`.
    public var reason: LoopReason?
    public var lastCheck: LoopCheck?

    public init(
        check: String, maxAttempts: UInt32, timeout: UInt64, state: LoopState, attempts: UInt32,
        armedAt: UInt64, since: UInt64, reason: LoopReason? = nil, lastCheck: LoopCheck? = nil
    ) {
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

    /// Paused for the user, or stopped for a reason the user did not give. The
    /// Inbox puts the task in Needs attention (ADR 0019).
    public var needsUser: Bool {
        switch state {
        case .paused: true
        case .stopped: reason != .user && reason != .status
        case .armed, .checking, .passed, .other: false
        }
    }
}

/// A state added by a newer bridge decodes as `other`, with no known meaning.
public enum LoopState: Equatable, Hashable, Sendable {
    /// Waits for the agent's next finished turn.
    case armed
    case checking
    /// The agent waits for the user; the loop goes on after the answer.
    case paused
    case passed
    case stopped
    case other(String)

    private static let names: [(LoopState, String)] = [
        (.armed, "armed"), (.checking, "checking"), (.paused, "paused"), (.passed, "passed"),
        (.stopped, "stopped"),
    ]

    public init(raw: String) {
        self = Self.names.first { $0.1 == raw }?.0 ?? .other(raw)
    }

    public var raw: String {
        if case .other(let raw) = self { return raw }
        return Self.names.first { $0.0 == self }?.1 ?? ""
    }
}

/// Why a loop is `paused` or `stopped`. A reason added by a newer bridge decodes
/// as `other`.
public enum LoopReason: Equatable, Hashable, Sendable {
    /// The agent waits for the user (paused).
    case waiting
    /// `maxAttempts` checks failed.
    case limit
    /// A check ran longer than `timeout`.
    case timeout
    /// The agent's state is `error`.
    case error
    /// No linked session, no live agent pane, or a shell in the foreground.
    case noAgent
    /// A prompt could not be typed within 30 seconds.
    case notSent
    /// The agent did not take the typed prompt.
    case stalled
    /// The user set the status to anything but `open`.
    case status
    /// The user stopped it.
    case user
    case other(String)

    private static let names: [(LoopReason, String)] = [
        (.waiting, "waiting"), (.limit, "limit"), (.timeout, "timeout"), (.error, "error"),
        (.noAgent, "no-agent"), (.notSent, "not-sent"), (.stalled, "stalled"), (.status, "status"),
        (.user, "user"),
    ]

    public init(raw: String) {
        self = Self.names.first { $0.1 == raw }?.0 ?? .other(raw)
    }

    public var raw: String {
        if case .other(let raw) = self { return raw }
        return Self.names.first { $0.0 == self }?.1 ?? ""
    }
}

/// The last check a loop ran.
public struct LoopCheck: Codable, Equatable, Sendable {
    public var attempt: UInt32
    /// Nil when the check timed out.
    public var exit: Int32?
    /// How long it ran.
    public var seconds: UInt64
    /// Epoch seconds when it ended.
    public var at: UInt64

    public init(attempt: UInt32, exit: Int32? = nil, seconds: UInt64, at: UInt64) {
        self.attempt = attempt
        self.exit = exit
        self.seconds = seconds
        self.at = at
    }
}

/// A prompt the host waits to type into the task's agent. It is in the record
/// so that the wait survives a restart of the host process (ADR 0019).
public struct PendingPrompt: Codable, Equatable, Sendable {
    public var kind: PendingPromptKind
    /// Epoch seconds when the wait began.
    public var since: UInt64
    /// The `id` of the run the prompt is for.
    public var run: UInt32

    public init(kind: PendingPromptKind, since: UInt64, run: UInt32) {
        self.kind = kind
        self.since = since
        self.run = run
    }
}

/// A kind added by a newer bridge decodes as `other`.
public enum PendingPromptKind: Equatable, Hashable, Sendable {
    /// The start prompt of `create-task`.
    case start
    /// The prompt of `restart-agent`.
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

extension LoopState: Codable {
    public init(from decoder: any Decoder) throws {
        self.init(raw: try decoder.singleValueContainer().decode(String.self))
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.singleValueContainer()
        try c.encode(raw)
    }
}

extension LoopReason: Codable {
    public init(from decoder: any Decoder) throws {
        self.init(raw: try decoder.singleValueContainer().decode(String.self))
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.singleValueContainer()
        try c.encode(raw)
    }
}

extension PendingPromptKind: Codable {
    public init(from decoder: any Decoder) throws {
        self.init(raw: try decoder.singleValueContainer().decode(String.self))
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.singleValueContainer()
        try c.encode(raw)
    }
}
