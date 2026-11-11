import Foundation

// Delegate notes, the overall assistant and its plans (ADR 0049), at `work` level 10.
// A plan never changes a Work item, an Inbox item or their order.

/// One overall run's plan, from `plan-saved` or `inbox-list`: at most 3 rows to do next.
public struct Plan: Equatable, Sendable, Identifiable {
    /// `p-` and 8 hex digits.
    public var id: String
    /// Grows by one on every write; `apply-notes` sends it as `version`.
    public var rev: UInt64
    /// Epoch seconds: the schedule's time, or when `plan-now` asked.
    public var at: UInt64
    /// Epoch seconds: when the host read the inputs for the brief.
    public var inputsAt: UInt64
    public var delivery: Delivery
    /// Epoch seconds: when `delivery` was last derived.
    public var deliveryAt: UInt64
    /// The time the user has, from `plan-now`.
    public var minutes: UInt32?
    /// The run record, `t-…` of kind `assistant-run`.
    public var run: String?
    /// The workspaces whose data the brief carried.
    public var workspaces: [String]
    /// At most 3, pinned Work items first.
    public var top: [PlanRow]
    /// Work items moved down with Later whose `due` is within 24 hours.
    public var dueSoon: [String]
    /// Derived when listed: requests whose run has not answered.
    public var unanswered: [String]
    /// Derived when listed: requests whose newest result is older than their Work item.
    public var stale: [String]
    /// Derived against the previous plan.
    public var changed: [PlanChange]
    /// Level 11: Today's rows in the user's order, from `update-plan reorder`.
    public var userOrder: [String]
    /// Level 11: rows the user removed from Today, from `update-plan dismiss`.
    public var dismissed: [String]
    /// The overall assistant's proposed notes.
    public var notes: Notes?
    /// Why a plan is `failed` or `cancelled`: host text naming ids and limits.
    public var reason: String?

    public init(
        id: String, rev: UInt64, at: UInt64, inputsAt: UInt64, delivery: Delivery, deliveryAt: UInt64,
        minutes: UInt32? = nil, run: String? = nil, workspaces: [String] = [], top: [PlanRow] = [],
        dueSoon: [String] = [], unanswered: [String] = [], stale: [String] = [], changed: [PlanChange] = [],
        userOrder: [String] = [], dismissed: [String] = [], notes: Notes? = nil, reason: String? = nil
    ) {
        self.id = id
        self.rev = rev
        self.at = at
        self.inputsAt = inputsAt
        self.delivery = delivery
        self.deliveryAt = deliveryAt
        self.minutes = minutes
        self.run = run
        self.workspaces = workspaces
        self.top = top
        self.dueSoon = dueSoon
        self.unanswered = unanswered
        self.stale = stale
        self.changed = changed
        self.userOrder = userOrder
        self.dismissed = dismissed
        self.notes = notes
        self.reason = reason
    }
}

extension Plan: Codable {
    private enum Key: String, CodingKey {
        case id, rev, at, inputsAt, delivery, deliveryAt, minutes, run, workspaces, top, dueSoon, unanswered, stale
        case changed, userOrder, dismissed, notes, reason
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: Key.self)
        id = try c.decode(String.self, forKey: .id)
        rev = try c.decode(UInt64.self, forKey: .rev)
        at = try c.decode(UInt64.self, forKey: .at)
        inputsAt = try c.decode(UInt64.self, forKey: .inputsAt)
        delivery = try c.decode(Delivery.self, forKey: .delivery)
        deliveryAt = try c.decode(UInt64.self, forKey: .deliveryAt)
        minutes = try c.decodeIfPresent(UInt32.self, forKey: .minutes)
        run = try c.decodeIfPresent(String.self, forKey: .run)
        workspaces = try c.decodeIfPresent([String].self, forKey: .workspaces) ?? []
        top = try c.decodeIfPresent([PlanRow].self, forKey: .top) ?? []
        dueSoon = try c.decodeIfPresent([String].self, forKey: .dueSoon) ?? []
        unanswered = try c.decodeIfPresent([String].self, forKey: .unanswered) ?? []
        stale = try c.decodeIfPresent([String].self, forKey: .stale) ?? []
        changed = try c.decodeIfPresent([PlanChange].self, forKey: .changed) ?? []
        userOrder = try c.decodeIfPresent([String].self, forKey: .userOrder) ?? []
        dismissed = try c.decodeIfPresent([String].self, forKey: .dismissed) ?? []
        notes = try c.decodeIfPresent(Notes.self, forKey: .notes)
        reason = try c.decodeIfPresent(String.self, forKey: .reason)
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.container(keyedBy: Key.self)
        try c.encode(id, forKey: .id)
        try c.encode(rev, forKey: .rev)
        try c.encode(at, forKey: .at)
        try c.encode(inputsAt, forKey: .inputsAt)
        try c.encode(delivery, forKey: .delivery)
        try c.encode(deliveryAt, forKey: .deliveryAt)
        try c.encodeIfPresent(minutes, forKey: .minutes)
        try c.encodeIfPresent(run, forKey: .run)
        if !workspaces.isEmpty { try c.encode(workspaces, forKey: .workspaces) }
        if !top.isEmpty { try c.encode(top, forKey: .top) }
        if !dueSoon.isEmpty { try c.encode(dueSoon, forKey: .dueSoon) }
        if !unanswered.isEmpty { try c.encode(unanswered, forKey: .unanswered) }
        if !stale.isEmpty { try c.encode(stale, forKey: .stale) }
        if !changed.isEmpty { try c.encode(changed, forKey: .changed) }
        if !userOrder.isEmpty { try c.encode(userOrder, forKey: .userOrder) }
        if !dismissed.isEmpty { try c.encode(dismissed, forKey: .dismissed) }
        try c.encodeIfPresent(notes, forKey: .notes)
        try c.encodeIfPresent(reason, forKey: .reason)
    }
}

/// One row of a plan's `top`: a Work item or an Inbox item.
public struct PlanRow: Equatable, Sendable {
    public var work: String?
    public var item: String?
    public var why: String
    public var next: String
    public var minutes: Minutes
    public var basis: [Basis]
    /// What the agent would need to know to rank it better.
    public var missing: [String]
    /// Derived when listed: the Work item's title or the Inbox item's question.
    public var title: String?
    /// Derived when listed: the target's state when it is no longer open. `nil`: open.
    public var targetState: TargetState?

    public init(
        work: String? = nil, item: String? = nil, why: String, next: String, minutes: Minutes, basis: [Basis] = [],
        missing: [String] = [], title: String? = nil, targetState: TargetState? = nil
    ) {
        self.work = work
        self.item = item
        self.why = why
        self.next = next
        self.minutes = minutes
        self.basis = basis
        self.missing = missing
        self.title = title
        self.targetState = targetState
    }
}

extension PlanRow: Codable {
    private enum Key: String, CodingKey { case work, item, why, next, minutes, basis, missing, title, targetState }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: Key.self)
        work = try c.decodeIfPresent(String.self, forKey: .work)
        item = try c.decodeIfPresent(String.self, forKey: .item)
        why = try c.decode(String.self, forKey: .why)
        next = try c.decode(String.self, forKey: .next)
        minutes = try c.decode(Minutes.self, forKey: .minutes)
        basis = try c.decodeIfPresent([Basis].self, forKey: .basis) ?? []
        missing = try c.decodeIfPresent([String].self, forKey: .missing) ?? []
        title = try c.decodeIfPresent(String.self, forKey: .title)
        targetState = try c.decodeIfPresent(TargetState.self, forKey: .targetState)
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.container(keyedBy: Key.self)
        try c.encodeIfPresent(work, forKey: .work)
        try c.encodeIfPresent(item, forKey: .item)
        try c.encode(why, forKey: .why)
        try c.encode(next, forKey: .next)
        try c.encode(minutes, forKey: .minutes)
        if !basis.isEmpty { try c.encode(basis, forKey: .basis) }
        if !missing.isEmpty { try c.encode(missing, forKey: .missing) }
        try c.encodeIfPresent(title, forKey: .title)
        try c.encodeIfPresent(targetState, forKey: .targetState)
    }
}

/// The agent's estimate of the time a row takes: `low <= estimated <= high`.
public struct Minutes: Codable, Equatable, Sendable {
    public var low: UInt32
    public var high: UInt32
    public var estimated: UInt32

    public init(low: UInt32, high: UInt32, estimated: UInt32) {
        self.low = low
        self.high = high
        self.estimated = estimated
    }
}

/// One reason a row is in the plan.
public struct Basis: Codable, Equatable, Sendable {
    public var kind: BasisKind
    public var text: String
    /// A URL, a path or an id the reason comes from.
    public var source: String?
    /// Epoch seconds, e.g. an estimated deadline.
    public var at: UInt64?

    public init(kind: BasisKind, text: String, source: String? = nil, at: UInt64? = nil) {
        self.kind = kind
        self.text = text
        self.source = source
        self.at = at
    }
}

/// A kind added by a newer bridge decodes as `other`.
public enum BasisKind: Equatable, Hashable, Sendable {
    case due, dependency, goal, time
    /// The user's pin.
    case manual
    /// Level 11: a policy, a hypothesis or a correction, by id in `source` (ADR 0050).
    case policy, hypothesis, correction
    case other(String)

    private static let names: [(BasisKind, String)] = [
        (.due, "due"), (.dependency, "dependency"), (.goal, "goal"), (.time, "time"), (.manual, "manual"),
        (.policy, "policy"), (.hypothesis, "hypothesis"), (.correction, "correction"),
    ]

    public init(raw: String) {
        self = Self.names.first { $0.1 == raw }?.0 ?? .other(raw)
    }

    public var raw: String {
        if case .other(let raw) = self { return raw }
        return Self.names.first { $0.0 == self }?.1 ?? ""
    }
}

/// A row that entered, left or moved since the previous plan.
public struct PlanChange: Codable, Equatable, Sendable {
    /// The Work item or Inbox item.
    public var id: String
    public var change: ChangeKind

    public init(id: String, change: ChangeKind) {
        self.id = id
        self.change = change
    }
}

/// A change added by a newer bridge decodes as `other`.
public enum ChangeKind: Equatable, Hashable, Sendable {
    case entered, left, moved
    case other(String)

    private static let names: [(ChangeKind, String)] = [(.entered, "entered"), (.left, "left"), (.moved, "moved")]

    public init(raw: String) {
        self = Self.names.first { $0.1 == raw }?.0 ?? .other(raw)
    }

    public var raw: String {
        if case .other(let raw) = self { return raw }
        return Self.names.first { $0.0 == self }?.1 ?? ""
    }
}

/// A row's target that is no longer open. An unknown value is a state a newer host added.
public enum TargetState: Equatable, Hashable, Sendable {
    case answered, resolved, done, withdrawn, gone
    case other(String)

    private static let names: [(TargetState, String)] = [
        (.answered, "answered"), (.resolved, "resolved"), (.done, "done"), (.withdrawn, "withdrawn"), (.gone, "gone"),
    ]

    public init(raw: String) {
        self = Self.names.first { $0.1 == raw }?.0 ?? .other(raw)
    }

    public var raw: String {
        if case .other(let raw) = self { return raw }
        return Self.names.first { $0.0 == self }?.1 ?? ""
    }
}

/// A `sort` Inbox item's action: what `sort-capture` would do.
public struct SortProposal: Codable, Equatable, Sendable {
    public var capture: String
    public var action: SortAction
    public var workspace: String?
    public var title: String?
    /// With `link`.
    public var work: String?

    public init(
        capture: String, action: SortAction, workspace: String? = nil, title: String? = nil, work: String? = nil
    ) {
        self.capture = capture
        self.action = action
        self.workspace = workspace
        self.title = title
        self.work = work
    }
}

/// A `delegate` Inbox item's action: what `request-assistant` would do.
public struct DelegateProposal: Codable, Equatable, Sendable {
    public var work: String
    /// The receiving workspace.
    public var workspace: String
    public var scope: AssistScope
    /// The ask, at most 1 KiB.
    public var purpose: String
    /// Why the overall assistant asks.
    public var reason: String
    /// Level 12: the staff member to ask. Nil: the workspace's lead.
    public var worker: String?

    public init(
        work: String, workspace: String, scope: AssistScope, purpose: String, reason: String, worker: String? = nil
    ) {
        self.work = work
        self.workspace = workspace
        self.scope = scope
        self.purpose = purpose
        self.reason = reason
        self.worker = worker
    }
}

/// Who assists on this host, in `inbox-list`.
public struct AssistantInfo: Equatable, Sendable {
    /// The overall assistant's workspace. Leave it out of workspace pickers.
    public var overall: String?
    public var delegates: [DelegateInfo]

    public init(overall: String? = nil, delegates: [DelegateInfo] = []) {
        self.overall = overall
        self.delegates = delegates
    }
}

extension AssistantInfo: Codable {
    private enum Key: String, CodingKey { case overall, delegates }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: Key.self)
        overall = try c.decodeIfPresent(String.self, forKey: .overall)
        delegates = try c.decodeIfPresent([DelegateInfo].self, forKey: .delegates) ?? []
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.container(keyedBy: Key.self)
        try c.encodeIfPresent(overall, forKey: .overall)
        if !delegates.isEmpty { try c.encode(delegates, forKey: .delegates) }
    }
}

/// A workspace with `[workspace.assistant]`.
public struct DelegateInfo: Equatable, Sendable {
    public var workspace: String
    public var share: Share
    public var overallRequests: OverallRequests
    /// Epoch seconds: when its `NOTES.md` last changed. Nil: no notes.
    public var notesAt: UInt64?
    /// Level 12: who answers its requests, the lead marked (ADR 0051 §1).
    public var staff: [StaffMember]
    /// Level 13: the worker that performs approved writes (ADR 0052 §3).
    public var writer: WriterInfo?
    /// Level 13: whether the workspace takes write proposals. Nil: an older host.
    public var writes: Writes?

    public init(
        workspace: String, share: Share, overallRequests: OverallRequests, notesAt: UInt64? = nil,
        staff: [StaffMember] = [], writer: WriterInfo? = nil, writes: Writes? = nil
    ) {
        self.workspace = workspace
        self.share = share
        self.overallRequests = overallRequests
        self.notesAt = notesAt
        self.staff = staff
        self.writer = writer
        self.writes = writes
    }
}

extension DelegateInfo: Codable {
    private enum Key: String, CodingKey { case workspace, share, overallRequests, notesAt, staff, writer, writes }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: Key.self)
        workspace = try c.decode(String.self, forKey: .workspace)
        share = try c.decode(Share.self, forKey: .share)
        overallRequests = try c.decode(OverallRequests.self, forKey: .overallRequests)
        notesAt = try c.decodeIfPresent(UInt64.self, forKey: .notesAt)
        staff = try c.decodeIfPresent([StaffMember].self, forKey: .staff) ?? []
        writer = try c.decodeIfPresent(WriterInfo.self, forKey: .writer)
        writes = try c.decodeIfPresent(Writes.self, forKey: .writes)
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.container(keyedBy: Key.self)
        try c.encode(workspace, forKey: .workspace)
        try c.encode(share, forKey: .share)
        try c.encode(overallRequests, forKey: .overallRequests)
        try c.encodeIfPresent(notesAt, forKey: .notesAt)
        if !staff.isEmpty { try c.encode(staff, forKey: .staff) }
        try c.encodeIfPresent(writer, forKey: .writer)
        try c.encodeIfPresent(writes, forKey: .writes)
    }
}

/// What a workspace shares with the overall assistant (ADR 0049 §3). A value added by
/// a newer bridge decodes as `other`.
public enum Share: Equatable, Hashable, Sendable {
    case none, outline, summaries
    case other(String)

    private static let names: [(Share, String)] = [(.none, "none"), (.outline, "outline"), (.summaries, "summaries")]

    public init(raw: String) {
        self = Self.names.first { $0.1 == raw }?.0 ?? .other(raw)
    }

    public var raw: String {
        if case .other(let raw) = self { return raw }
        return Self.names.first { $0.0 == self }?.1 ?? ""
    }
}

/// Whether the overall assistant may ask this workspace (ADR 0049 §4). A value added by
/// a newer bridge decodes as `other`.
public enum OverallRequests: Equatable, Hashable, Sendable {
    /// Refuses even approved requests.
    case none
    /// The user approves each one.
    case review
    /// Sent without approval, within every budget.
    case auto
    case other(String)

    private static let names: [(OverallRequests, String)] = [(.none, "none"), (.review, "review"), (.auto, "auto")]

    public init(raw: String) {
        self = Self.names.first { $0.1 == raw }?.0 ?? .other(raw)
    }

    public var raw: String {
        if case .other(let raw) = self { return raw }
        return Self.names.first { $0.0 == self }?.1 ?? ""
    }
}

extension BasisKind: Codable {
    public init(from decoder: any Decoder) throws {
        self.init(raw: try decoder.singleValueContainer().decode(String.self))
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.singleValueContainer()
        try c.encode(raw)
    }
}

extension ChangeKind: Codable {
    public init(from decoder: any Decoder) throws {
        self.init(raw: try decoder.singleValueContainer().decode(String.self))
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.singleValueContainer()
        try c.encode(raw)
    }
}

extension TargetState: Codable {
    public init(from decoder: any Decoder) throws {
        self.init(raw: try decoder.singleValueContainer().decode(String.self))
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.singleValueContainer()
        try c.encode(raw)
    }
}

extension Share: Codable {
    public init(from decoder: any Decoder) throws {
        self.init(raw: try decoder.singleValueContainer().decode(String.self))
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.singleValueContainer()
        try c.encode(raw)
    }
}

extension OverallRequests: Codable {
    public init(from decoder: any Decoder) throws {
        self.init(raw: try decoder.singleValueContainer().decode(String.self))
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.singleValueContainer()
        try c.encode(raw)
    }
}
