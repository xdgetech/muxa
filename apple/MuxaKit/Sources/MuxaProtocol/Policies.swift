import Foundation

// Policies, corrections and hypotheses (ADR 0048), at `work` level 10: what the user
// told the overall assistant, and what the host saw.

/// The user's own words, which outrank every hypothesis and note.
public struct Policy: Codable, Equatable, Sendable, Identifiable {
    /// `y-` and 8 hex digits.
    public var id: String
    public var rev: UInt64
    /// 1 to 500 bytes, one paragraph.
    public var text: String
    /// `all`, or one workspace's name.
    public var scope: String
    /// Epoch seconds.
    public var from: UInt64?
    /// Epoch seconds. Past it the policy is expired and in no brief.
    public var until: UInt64?
    /// `user`, or the `h-` hypothesis it was confirmed from.
    public var source: String
    public var createdAt: UInt64
    public var updatedAt: UInt64

    public init(
        id: String, rev: UInt64, text: String, scope: String, from: UInt64? = nil, until: UInt64? = nil,
        source: String, createdAt: UInt64, updatedAt: UInt64
    ) {
        self.id = id
        self.rev = rev
        self.text = text
        self.scope = scope
        self.from = from
        self.until = until
        self.source = source
        self.createdAt = createdAt
        self.updatedAt = updatedAt
    }
}

/// A user's action on a plan, as the host saw it.
public struct Correction: Equatable, Sendable, Identifiable {
    /// `x-` and 8 hex digits.
    public var id: String
    public var kind: CorrectionKind
    /// Epoch seconds.
    public var at: UInt64
    /// The plan that was current.
    public var plan: String
    /// The Work item or Inbox item. Nil with `reordered`.
    public var row: String?
    /// Whether the plan's `top` listed the row.
    public var inTop: Bool
    /// The row's workspace; nil for nobody's items.
    public var workspace: String?
    /// With `reordered`: the rows' order before and after.
    public var before: [String]
    public var after: [String]
    /// The user's optional reason, at most 500 bytes.
    public var reason: String?

    public init(
        id: String, kind: CorrectionKind, at: UInt64, plan: String, row: String? = nil, inTop: Bool,
        workspace: String? = nil, before: [String] = [], after: [String] = [], reason: String? = nil
    ) {
        self.id = id
        self.kind = kind
        self.at = at
        self.plan = plan
        self.row = row
        self.inTop = inTop
        self.workspace = workspace
        self.before = before
        self.after = after
        self.reason = reason
    }
}

extension Correction: Codable {
    private enum Key: String, CodingKey { case id, kind, at, plan, row, inTop, workspace, before, after, reason }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: Key.self)
        id = try c.decode(String.self, forKey: .id)
        kind = try c.decode(CorrectionKind.self, forKey: .kind)
        at = try c.decode(UInt64.self, forKey: .at)
        plan = try c.decode(String.self, forKey: .plan)
        row = try c.decodeIfPresent(String.self, forKey: .row)
        inTop = try c.decode(Bool.self, forKey: .inTop)
        workspace = try c.decodeIfPresent(String.self, forKey: .workspace)
        before = try c.decodeIfPresent([String].self, forKey: .before) ?? []
        after = try c.decodeIfPresent([String].self, forKey: .after) ?? []
        reason = try c.decodeIfPresent(String.self, forKey: .reason)
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.container(keyedBy: Key.self)
        try c.encode(id, forKey: .id)
        try c.encode(kind, forKey: .kind)
        try c.encode(at, forKey: .at)
        try c.encode(plan, forKey: .plan)
        try c.encodeIfPresent(row, forKey: .row)
        try c.encode(inTop, forKey: .inTop)
        try c.encodeIfPresent(workspace, forKey: .workspace)
        if !before.isEmpty { try c.encode(before, forKey: .before) }
        if !after.isEmpty { try c.encode(after, forKey: .after) }
        try c.encodeIfPresent(reason, forKey: .reason)
    }
}

/// A kind added by a newer bridge decodes as `other`.
public enum CorrectionKind: Equatable, Hashable, Sendable {
    /// Today's rows dragged into another order.
    case reordered
    /// A row removed from Today ("Not today").
    case dismissed
    /// A `sort`, `delegate` or `hypothesis` item rejected.
    case rejected
    case pinned, unpinned
    /// Later: "not now", never "unimportant".
    case deferred
    case other(String)

    private static let names: [(CorrectionKind, String)] = [
        (.reordered, "reordered"), (.dismissed, "dismissed"), (.rejected, "rejected"), (.pinned, "pinned"),
        (.unpinned, "unpinned"), (.deferred, "deferred"),
    ]

    public init(raw: String) {
        self = Self.names.first { $0.1 == raw }?.0 ?? .other(raw)
    }

    public var raw: String {
        if case .other(let raw) = self { return raw }
        return Self.names.first { $0.0 == self }?.1 ?? ""
    }
}

/// The overall assistant's guess at a preference, which the user confirms or rejects.
public struct Hypothesis: Codable, Equatable, Sendable, Identifiable {
    /// `h-` and 8 hex digits.
    public var id: String
    /// 1 to 500 bytes.
    public var text: String
    /// `all`, or one workspace's name.
    public var scope: String
    /// 1 to 5 correction ids.
    public var evidence: [String]
    public var confidence: Confidence
    public var state: HypothesisState
    /// The plan that proposed it.
    public var plan: String
    /// Epoch seconds: when a `proposed` one expires, or a `rejected` one leaves the brief.
    public var expiresAt: UInt64
    /// Epoch seconds: when it was proposed.
    public var at: UInt64

    public init(
        id: String, text: String, scope: String, evidence: [String], confidence: Confidence,
        state: HypothesisState, plan: String, expiresAt: UInt64, at: UInt64
    ) {
        self.id = id
        self.text = text
        self.scope = scope
        self.evidence = evidence
        self.confidence = confidence
        self.state = state
        self.plan = plan
        self.expiresAt = expiresAt
        self.at = at
    }
}

/// A value added by a newer bridge decodes as `other`.
public enum Confidence: Equatable, Hashable, Sendable {
    case low, medium
    case other(String)

    private static let names: [(Confidence, String)] = [(.low, "low"), (.medium, "medium")]

    public init(raw: String) {
        self = Self.names.first { $0.1 == raw }?.0 ?? .other(raw)
    }

    public var raw: String {
        if case .other(let raw) = self { return raw }
        return Self.names.first { $0.0 == self }?.1 ?? ""
    }
}

/// A state added by a newer bridge decodes as `other`.
public enum HypothesisState: Equatable, Hashable, Sendable {
    case proposed
    /// It became a policy.
    case confirmed
    case rejected
    /// Nobody answered it in 14 days.
    case expired
    case other(String)

    private static let names: [(HypothesisState, String)] = [
        (.proposed, "proposed"), (.confirmed, "confirmed"), (.rejected, "rejected"), (.expired, "expired"),
    ]

    public init(raw: String) {
        self = Self.names.first { $0.1 == raw }?.0 ?? .other(raw)
    }

    public var raw: String {
        if case .other(let raw) = self { return raw }
        return Self.names.first { $0.0 == self }?.1 ?? ""
    }
}

/// A `hypothesis` Inbox item's payload: what `confirm` makes a policy of.
public struct HypothesisProposal: Codable, Equatable, Sendable {
    /// The `h-` id.
    public var hypothesis: String
    public var text: String
    public var scope: String
    public var evidence: [String]
    public var confidence: Confidence

    public init(hypothesis: String, text: String, scope: String, evidence: [String], confidence: Confidence) {
        self.hypothesis = hypothesis
        self.text = text
        self.scope = scope
        self.evidence = evidence
        self.confidence = confidence
    }
}

/// What `update-policy` does.
public enum PolicyAction: String, Codable, Sendable {
    /// Change `text`, `scope`, `from` or `until`.
    case edit
    case delete
}

/// What `update-plan` does to the newest `resulted` plan.
public enum PlanAction: String, Codable, Sendable {
    /// Set `userOrder` to `order`.
    case reorder
    /// Add `row` to `dismissed`, with `text` as the reason.
    case dismiss
}

extension CorrectionKind: Codable {
    public init(from decoder: any Decoder) throws {
        self.init(raw: try decoder.singleValueContainer().decode(String.self))
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.singleValueContainer()
        try c.encode(raw)
    }
}

extension Confidence: Codable {
    public init(from decoder: any Decoder) throws {
        self.init(raw: try decoder.singleValueContainer().decode(String.self))
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.singleValueContainer()
        try c.encode(raw)
    }
}

extension HypothesisState: Codable {
    public init(from decoder: any Decoder) throws {
        self.init(raw: try decoder.singleValueContainer().decode(String.self))
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.singleValueContainer()
        try c.encode(raw)
    }
}
