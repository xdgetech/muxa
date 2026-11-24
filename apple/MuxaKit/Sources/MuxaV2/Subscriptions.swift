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
