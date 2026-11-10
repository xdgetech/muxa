import Foundation

// Staff and hand-offs inside a workspace (ADR 0051), at `work` level 12. A staff member is a
// host `[[worker]]`; a request version runs as a chain of steps, one run each.

/// One run of a request version's chain. Step 1 is the first member asked.
public struct RequestStep: Codable, Equatable, Sendable {
    /// From 1.
    public var step: UInt32
    /// The staff member who runs it: a host `[[worker]]` name.
    public var worker: String
    public var by: StepBy
    /// As a request's, for this step's run.
    public var delivery: Delivery
    /// Epoch seconds: when `delivery` was last derived.
    public var deliveryAt: UInt64
    /// The step's run record, `t-…` of kind `assistant-run`.
    public var run: String?
    /// The hand-off this step's result asks for: the next step, once approved.
    public var handoff: Handoff?

    public init(
        step: UInt32, worker: String, by: StepBy, delivery: Delivery, deliveryAt: UInt64, run: String? = nil,
        handoff: Handoff? = nil
    ) {
        self.step = step
        self.worker = worker
        self.by = by
        self.delivery = delivery
        self.deliveryAt = deliveryAt
        self.run = run
        self.handoff = handoff
    }
}

/// Who picked a step's worker. A value added by a newer bridge decodes as `other`: show it as unknown.
public enum StepBy: Equatable, Hashable, Sendable {
    /// The user named the worker.
    case user
    /// The overall assistant named it in a `delegate` proposal.
    case overall
    /// Nobody named one: the workspace's lead took it.
    case lead
    /// The previous step handed it on.
    case handoff
    case other(String)

    private static let names: [(StepBy, String)] = [
        (.user, "user"), (.overall, "overall"), (.lead, "lead"), (.handoff, "handoff"),
    ]

    public init(raw: String) {
        self = Self.names.first { $0.1 == raw }?.0 ?? .other(raw)
    }

    public var raw: String {
        if case .other(let raw) = self { return raw }
        return Self.names.first { $0.0 == self }?.1 ?? ""
    }
}

extension StepBy: Codable {
    public init(from decoder: any Decoder) throws {
        self.init(raw: try decoder.singleValueContainer().decode(String.self))
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.singleValueContainer()
        try c.encode(raw)
    }
}

/// "Pass this to that member next." Its purpose is never logged.
public struct Handoff: Codable, Equatable, Sendable {
    /// A staff member of the same workspace, not yet in the chain.
    public var worker: String
    /// What the next member does within the request's scope, 1 byte to 1 KiB.
    public var purpose: String

    public init(worker: String, purpose: String) {
        self.worker = worker
        self.purpose = purpose
    }
}

/// A `handoff` Inbox item's payload: what approve starts.
public struct HandoffProposal: Codable, Equatable, Sendable {
    /// `q-…`.
    public var request: String
    public var version: UInt64
    /// The step whose result asks for the hand-off; approve starts `step + 1`.
    public var step: UInt32
    /// The Work item, to open its detail.
    public var work: String
    /// The worker of `step`.
    public var from: String
    /// The next worker.
    public var worker: String
    /// Never logged.
    public var purpose: String

    public init(
        request: String, version: UInt64, step: UInt32, work: String, from: String, worker: String, purpose: String
    ) {
        self.request = request
        self.version = version
        self.step = step
        self.work = work
        self.from = from
        self.worker = worker
        self.purpose = purpose
    }
}

/// A workspace's staff member, in `inbox-list`'s `assistant.delegates[]`.
public struct StaffMember: Equatable, Sendable {
    /// A host `[[worker]]` name.
    public var worker: String
    /// The worker's job description, at most 1 KiB.
    public var description: String
    /// The member who takes requests that name no worker.
    public var lead: Bool
    /// Epoch seconds: when the member's own notes last changed. Nil: none.
    public var notesAt: UInt64?

    public init(worker: String, description: String, lead: Bool = false, notesAt: UInt64? = nil) {
        self.worker = worker
        self.description = description
        self.lead = lead
        self.notesAt = notesAt
    }
}

extension StaffMember: Codable {
    private enum Key: String, CodingKey { case worker, description, lead, notesAt }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: Key.self)
        worker = try c.decode(String.self, forKey: .worker)
        description = try c.decode(String.self, forKey: .description)
        lead = try c.decodeIfPresent(Bool.self, forKey: .lead) ?? false
        notesAt = try c.decodeIfPresent(UInt64.self, forKey: .notesAt)
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.container(keyedBy: Key.self)
        try c.encode(worker, forKey: .worker)
        try c.encode(description, forKey: .description)
        if lead { try c.encode(true, forKey: .lead) }
        try c.encodeIfPresent(notesAt, forKey: .notesAt)
    }
}
