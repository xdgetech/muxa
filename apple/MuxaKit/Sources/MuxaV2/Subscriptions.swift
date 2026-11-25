import Foundation

/// A subscription, a named agent login, and the plan limits its agents last
/// reported (ADR 0091). No credential and no account.
public struct SubscriptionUsage: Codable, Equatable, Sendable, Identifiable {
    public var name: String
    /// `claude-code` or `codex`.
    public var agent: String
    /// A label only.
    public var plan: String?
    /// Shortest first; empty before its first report.
    public var windows: [UsageWindow]
    /// When the last report came; nil before the first.
    public var asOf: Date?

    public var id: String { name }

    public init(name: String, agent: String, plan: String? = nil, windows: [UsageWindow] = [], asOf: Date? = nil) {
        self.name = name
        self.agent = agent
        self.plan = plan
        self.windows = windows
        self.asOf = asOf
    }

    enum CodingKeys: String, CodingKey {
        case name, agent, plan, windows
        case asOf = "as_of"
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        name = try c.decode(String.self, forKey: .name)
        agent = try c.decode(String.self, forKey: .agent)
        plan = try c.decodeIfPresent(String.self, forKey: .plan)
        windows = try c.decode(.windows, or: [])
        asOf = try c.decodeIfPresent(UInt64.self, forKey: .asOf).map { Date(timeIntervalSince1970: TimeInterval($0)) }
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encode(name, forKey: .name)
        try c.encode(agent, forKey: .agent)
        try c.encodeIfPresent(plan, forKey: .plan)
        try c.encode(windows, forKey: .windows)
        try c.encodeIfPresent(asOf.map { UInt64($0.timeIntervalSince1970) }, forKey: .asOf)
    }
}

/// One plan window, such as 5 hours (300 minutes) or a week (10080).
public struct UsageWindow: Codable, Equatable, Sendable {
    public var minutes: UInt32
    /// 0 to 100.
    public var usedPercent: UInt32
    public var resetsAt: Date

    public init(minutes: UInt32, usedPercent: UInt32, resetsAt: Date) {
        self.minutes = minutes
        self.usedPercent = usedPercent
        self.resetsAt = resetsAt
    }

    /// The window reset after its last report: its percent is no longer known.
    public func hasReset(now: Date = Date()) -> Bool { resetsAt <= now }

    enum CodingKeys: String, CodingKey {
        case minutes
        case usedPercent = "used_percent"
        case resetsAt = "resets_at"
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        minutes = try c.decode(UInt32.self, forKey: .minutes)
        usedPercent = try c.decode(UInt32.self, forKey: .usedPercent)
        resetsAt = Date(timeIntervalSince1970: TimeInterval(try c.decode(UInt64.self, forKey: .resetsAt)))
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encode(minutes, forKey: .minutes)
        try c.encode(usedPercent, forKey: .usedPercent)
        try c.encode(UInt64(resetsAt.timeIntervalSince1970), forKey: .resetsAt)
    }
}

/// The tokens agents used on one day on the host's clock, from their own
/// transcripts (ADR 0091 §6). Tokens only, never a share of a plan.
public struct TokenUsage: Codable, Equatable, Sendable {
    /// `YYYY-MM-DD`.
    public var day: String
    /// Most first.
    public var workspaces: [WorkspaceTokens]
    /// The folders inside a Workspace other than its root, most first.
    public var tasks: [TaskTokens]

    public init(day: String, workspaces: [WorkspaceTokens] = [], tasks: [TaskTokens] = []) {
        self.day = day
        self.workspaces = workspaces
        self.tasks = tasks
    }
}

public struct WorkspaceTokens: Codable, Equatable, Sendable, Identifiable {
    public var workspace: String
    /// Input not read from a cache.
    public var input: UInt64
    /// Input read from a cache.
    public var cached: UInt64
    /// Output, reasoning included.
    public var output: UInt64

    public var id: String { workspace }

    public init(workspace: String, input: UInt64, cached: UInt64 = 0, output: UInt64) {
        self.workspace = workspace
        self.input = input
        self.cached = cached
        self.output = output
    }
}

public struct TaskTokens: Codable, Equatable, Sendable, Identifiable {
    public var workspace: String
    /// The working folder, as a Task agent's `muxa.dir` names it.
    public var dir: String
    /// The folder's name.
    public var title: String
    public var input: UInt64
    public var cached: UInt64
    public var output: UInt64

    public var id: String { dir }

    public init(workspace: String, dir: String, title: String, input: UInt64, cached: UInt64 = 0, output: UInt64) {
        self.workspace = workspace
        self.dir = dir
        self.title = title
        self.input = input
        self.cached = cached
        self.output = output
    }
}

/// How token counts read: `84`, `84k`, `1.2M`.
public enum TokenText {
    public static func short(_ n: UInt64) -> String {
        switch n {
        case 0..<1_000: "\(n)"
        case 1_000..<1_000_000: rounded(Double(n) / 1_000) + "k"
        default: rounded(Double(n) / 1_000_000) + "M"
        }
    }

    /// `1.2M in · 84k out · 9.4M cached`.
    public static func line(input: UInt64, cached: UInt64, output: UInt64) -> String {
        var parts = ["\(short(input)) in", "\(short(output)) out"]
        if cached > 0 { parts.append("\(short(cached)) cached") }
        return parts.joined(separator: " · ")
    }

    /// One decimal below 10, none from 10: `1.2`, `84`.
    private static func rounded(_ x: Double) -> String {
        x < 9.95 ? String(format: "%.1f", x) : String(format: "%.0f", x)
    }
}
