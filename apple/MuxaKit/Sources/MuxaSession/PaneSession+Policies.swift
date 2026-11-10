import Foundation
import MuxaProtocol

/// Policies, corrections and hypotheses (ADR 0050) on a host with `work` level 11: what
/// the user told the overall assistant, and what the host saw the user do on a plan.
extension PaneSession {
    public var canUsePolicies: Bool { workLevel >= workPolicies }

    /// Shown when the host keeps no policies, corrections or hypotheses.
    public static let policiesUnsupported = "This host keeps no policies. Update muxa on the host."
    /// Shown when Today changed on another device first.
    public static let todayConflict = "Today changed on the host. It shows the newest plan now. Try again."
    /// The longest policy text, hypothesis text and reason the host takes, in bytes.
    public nonisolated static let textLimit = 500
    /// What Forget leaves, said next to it (ADR 0050 §5).
    public nonisolated static let forgetNote = "Later plans no longer use it. Plans made before keep their reasons."

    /// The policies that a plan follows now: no `until`, or one still to come.
    public func activePolicies(now: UInt64 = UInt64(Date().timeIntervalSince1970)) -> [Policy] {
        policies.filter { !$0.isExpired(now: now) }
    }

    /// Policies whose `until` passed: in no brief, listed for 30 days.
    public func expiredPolicies(now: UInt64 = UInt64(Date().timeIntervalSince1970)) -> [Policy] {
        policies.filter { $0.isExpired(now: now) }
    }

    /// The open `hypothesis` Inbox item of a hypothesis, which Confirm and Reject answer.
    public func item(for hypothesis: Hypothesis) -> InboxItem? {
        inboxItems.first { $0.kind == .hypothesis && $0.state == .open && $0.hypothesis?.hypothesis == hypothesis.id }
    }

    /// Why a policy's text cannot be sent, or nil: 1 to 500 bytes, one paragraph.
    public nonisolated static func policyTextProblem(_ text: String) -> String? {
        let bytes = text.trimmingCharacters(in: .whitespaces).utf8.count
        if bytes == 0 { return "Write the policy." }
        if bytes > textLimit { return "A policy has at most \(textLimit) bytes." }
        if text.unicodeScalars.contains(where: { CharacterSet.controlCharacters.contains($0) }) {
            return "A policy is one paragraph, with no line breaks."
        }
        return nil
    }

    /// The end of the week that holds `now`: Sunday, 23:59:59 in `calendar`'s zone, as
    /// `muxa-cli policy add --until week` makes it.
    public nonisolated static func endOfWeek(_ now: Date, calendar: Calendar = .current) -> Date {
        let weekday = calendar.component(.weekday, from: now)
        let days = (8 - weekday) % 7
        let sunday = calendar.date(byAdding: .day, value: days, to: calendar.startOfDay(for: now)) ?? now
        return calendar.date(bySettingHour: 23, minute: 59, second: 59, of: sunday) ?? sunday
    }

    // MARK: Policies

    /// Keeps a policy in the user's words. `scope` is `all` or a workspace; `from` and
    /// `until` are epoch seconds. A 51st active policy is `conflict`, naming the limit.
    @discardableResult
    public func addPolicy(text: String, scope: String, from: UInt64? = nil, until: UInt64? = nil) async throws -> Policy {
        try requirePolicies()
        return try await savingPolicy { .addPolicy(id: $0, text: text, scope: scope, from: from, until: until) }
    }

    /// Sets the fields given. The host cannot clear `from` or `until` yet. A stale `rev`
    /// is `conflict`; the list is read again so the next try has the host's version.
    @discardableResult
    public func editPolicy(
        _ policy: Policy, text: String? = nil, scope: String? = nil, from: UInt64? = nil, until: UInt64? = nil
    ) async throws -> Policy {
        try requirePolicies()
        return try await savingPolicy {
            .updatePolicy(
                id: $0, policy: policy.id, rev: policy.rev, action: .edit, text: text, scope: scope, from: from, until: until)
        }
    }

    /// Deletes the user's policy. A policy is deleted, never forgotten.
    public func deletePolicy(_ policy: Policy) async throws {
        try requirePolicies()
        do {
            try await request { .updatePolicy(id: $0, policy: policy.id, rev: policy.rev, action: .delete) }
        } catch {
            await loadWork()
            throw error
        }
        policies.removeAll { $0.id == policy.id }
    }

    private func savingPolicy(_ make: (UInt64) -> Request) async throws -> Policy {
        var requestId: UInt64 = 0
        defer { savedPolicies[requestId] = nil }
        do {
            try await request { id in
                requestId = id
                return make(id)
            }
        } catch {
            await loadWork()
            throw error
        }
        guard let saved = savedPolicies[requestId] else {
            throw Failure.bridge(ErrorBody(code: .internal, message: "The bridge did not return the policy."))
        }
        return saved
    }

    // MARK: Today

    /// Today's rows in the user's order when they reordered them, else the plan's. A row
    /// the user order does not name keeps its place after the named ones.
    public func todayRows(of plan: Plan) -> [PlanRow] {
        guard !plan.userOrder.isEmpty else { return plan.top }
        let named = plan.userOrder.compactMap { id in plan.top.first { $0.rowId == id } }
        return named + plan.top.filter { row in !plan.userOrder.contains { $0 == row.rowId } }
    }

    /// The user removed the row from Today ("Not today").
    public func isDismissed(_ row: PlanRow, in plan: Plan) -> Bool {
        row.rowId.map(plan.dismissed.contains) ?? false
    }

    /// Sets Today's order: `order` names every row of the current plan once. The host
    /// records one `reordered` correction; `top` keeps the agent's order.
    @discardableResult
    public func reorderToday(_ order: [String]) async throws -> Plan {
        try requirePolicies()
        guard let plan = currentPlan else { throw Self.noPlan }
        return try await updatingPlan { .updatePlan(id: $0, plan: plan.id, rev: plan.rev, action: .reorder, order: order) }
    }

    /// Removes a row from Today, with the user's optional reason. The host records one
    /// `dismissed` correction.
    @discardableResult
    public func dismissFromToday(_ row: String, reason: String? = nil) async throws -> Plan {
        try requirePolicies()
        guard let plan = currentPlan else { throw Self.noPlan }
        let text = Self.reason(reason)
        return try await updatingPlan {
            .updatePlan(id: $0, plan: plan.id, rev: plan.rev, action: .dismiss, row: row, text: text)
        }
    }

    private static let noPlan = Failure.bridge(ErrorBody(code: .conflict, message: todayConflict))

    private func updatingPlan(_ make: (UInt64) -> Request) async throws -> Plan {
        var requestId: UInt64 = 0
        defer { savedPlans[requestId] = nil }
        do {
            try await request { id in
                requestId = id
                return make(id)
            }
        } catch Failure.bridge(let body) where body.code == .conflict {
            // Another device changed Today, or a newer plan came.
            await loadWork()
            throw Failure.bridge(ErrorBody(code: .conflict, message: Self.todayConflict))
        }
        guard let saved = savedPlans[requestId] else {
            throw Failure.bridge(ErrorBody(code: .internal, message: "The bridge did not return the plan."))
        }
        // The host recorded a correction: list it.
        await loadWork()
        return saved
    }

    // MARK: Hypotheses and corrections

    /// Rejects a `sort`, `delegate` or `hypothesis` item, with the user's optional reason.
    /// Only a level 11 host takes the reason.
    @discardableResult
    public func reject(_ item: InboxItem, reason: String?) async throws -> InboxItem {
        try await answer(item, .reject, text: canUsePolicies ? Self.reason(reason) : nil)
    }

    /// Confirms a `hypothesis` item: the host makes a policy of its text and scope.
    @discardableResult
    public func confirm(_ item: InboxItem) async throws -> InboxItem {
        try requirePolicies()
        return try await answer(item, .confirm)
    }

    /// Forgets a correction `x-…` or a hypothesis `h-…`. Forgetting a correction also
    /// removes the proposed hypotheses it grounds, so the Inbox is listed again.
    public func forget(_ record: String) async throws {
        try requirePolicies()
        do {
            try await request { .forget(id: $0, record: record) }
        } catch {
            await loadWork()
            throw error
        }
        corrections.removeAll { $0.id == record }
        hypotheses.removeAll { $0.id == record }
        await loadWork()
    }

    /// The record a `policy`, `hypothesis` or `correction` basis cites, or `forgotten`
    /// when the host no longer has it. Nil for other kinds, and below level 11.
    public func cited(_ basis: Basis) -> CitedRecord? {
        guard canUsePolicies else { return nil }
        let id = basis.source ?? ""
        switch basis.kind {
        case .policy: return policies.first { $0.id == id }.map(CitedRecord.policy) ?? .forgotten
        case .hypothesis: return hypotheses.first { $0.id == id }.map(CitedRecord.hypothesis) ?? .forgotten
        case .correction: return corrections.first { $0.id == id }.map(CitedRecord.correction) ?? .forgotten
        default: return nil
        }
    }

    func keepMemory(of inbox: InboxListing) {
        // An older host sends none of them.
        policies = canUsePolicies ? inbox.policies : []
        hypotheses = canUsePolicies ? inbox.hypotheses : []
        corrections = canUsePolicies ? inbox.corrections : []
    }

    /// A newer version of a listed policy replaces it; a new one goes last among the active ones.
    func keep(_ policy: Policy) {
        if let i = policies.firstIndex(where: { $0.id == policy.id }) {
            policies[i] = policy
        } else {
            let now = UInt64(Date().timeIntervalSince1970)
            let firstExpired = policies.firstIndex { $0.isExpired(now: now) } ?? policies.endIndex
            policies.insert(policy, at: firstExpired)
        }
    }

    /// An empty reason is no reason.
    private static func reason(_ text: String?) -> String? {
        guard let text = text?.trimmingCharacters(in: .whitespacesAndNewlines), !text.isEmpty else { return nil }
        return text
    }

    private func requirePolicies() throws {
        guard canUsePolicies else {
            throw Failure.bridge(ErrorBody(code: .unsupported, message: Self.policiesUnsupported))
        }
    }
}

/// What a plan row's `basis` cites, as the host lists it now.
public enum CitedRecord: Equatable, Sendable {
    case policy(Policy)
    case hypothesis(Hypothesis)
    case correction(Correction)
    /// The id has no record: the user forgot it, or it expired.
    case forgotten
}

extension Policy {
    /// `until` passed: the policy is in no brief.
    public func isExpired(now: UInt64 = UInt64(Date().timeIntervalSince1970)) -> Bool {
        until.map { $0 <= now } ?? false
    }
}

extension PlanRow {
    /// The Work item or Inbox item the row names, as `update-plan` names it.
    public var rowId: String? { work ?? item }
}
