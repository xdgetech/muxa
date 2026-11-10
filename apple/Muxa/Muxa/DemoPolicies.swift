import Foundation
import MuxaProtocol
import MuxaSession

/// The demo's policies, corrections and hypotheses (`work` level 11, ADR 0050), kept in
/// memory with the host's checks: 50 active policies, a stale `rev` conflicts, Today's
/// order names every row once, and forgetting a correction removes what it grounds.
struct DemoPolicies: Sendable {
    var policies: [Policy] = []
    /// Every state; newest first.
    var hypotheses: [Hypothesis] = []
    /// Newest first.
    var corrections: [Correction] = []
    /// The next record number.
    var next = 0x100

    static let activeLimit = 50
    /// An expired policy is listed this long, in seconds.
    static let expiredKept: UInt64 = 30 * 86_400

    static func id(_ prefix: String, _ n: Int) -> String { String(format: "\(prefix)-%08x", 0xa000 + n) }

    mutating func newId(_ prefix: String) -> String {
        next += 1
        return Self.id(prefix, next)
    }
}

extension DemoWork {
    private static var now: UInt64 { UInt64(Date().timeIntervalSince1970) }

    /// What `inbox-list` sends: active then expired policies, `proposed` and `rejected`
    /// hypotheses, and the newest 50 corrections.
    var listedMemory: (policies: [Policy], hypotheses: [Hypothesis], corrections: [Correction]) {
        let now = Self.now
        let active = memory.policies.filter { !$0.isExpired(now: now) }
        let expired = memory.policies.filter { p in p.isExpired(now: now) && (p.until ?? 0) + DemoPolicies.expiredKept > now }
        let open = memory.hypotheses.filter { $0.state == .proposed || $0.state == .rejected }
        return (active + expired, open, Array(memory.corrections.prefix(50)))
    }

    // MARK: Policies

    /// `add-policy`. `scopes` are the workspaces a policy may name: all but the overall one.
    mutating func addPolicy(
        text: String, scope: String, from: UInt64?, until: UInt64?, scopes: [String]
    ) throws(Refusal) -> Policy {
        try checkPolicy(text: text, scope: scope, from: from, until: until, scopes: scopes)
        let active = memory.policies.filter { !$0.isExpired(now: Self.now) }.count
        guard active < DemoPolicies.activeLimit else {
            throw Refusal(.conflict, "\(DemoPolicies.activeLimit) policies are active, the limit. Delete one first.")
        }
        let policy = Policy(id: memory.newId("y"), rev: 1, text: text, scope: scope, from: from, until: until,
                            source: "user", createdAt: Self.now, updatedAt: Self.now)
        memory.policies.append(policy)
        return policy
    }

    /// `update-policy`: the edited policy, or nil after `delete`.
    mutating func updatePolicy(
        _ id: String, rev: UInt64, action: PolicyAction, text: String?, scope: String?, from: UInt64?, until: UInt64?,
        scopes: [String]
    ) throws(Refusal) -> Policy? {
        guard let i = memory.policies.firstIndex(where: { $0.id == id }) else { throw Refusal(.badRequest, "no policy \(id)") }
        guard memory.policies[i].rev == rev else { throw Refusal(.conflict, "\(id) is at rev \(memory.policies[i].rev)") }
        if action == .delete {
            memory.policies.remove(at: i)
            return nil
        }
        guard text != nil || scope != nil || from != nil || until != nil else {
            throw Refusal(.badRequest, "edit needs text, scope, from or until")
        }
        var p = memory.policies[i]
        p.text = text ?? p.text
        p.scope = scope ?? p.scope
        p.from = from ?? p.from
        p.until = until ?? p.until
        try checkPolicy(text: p.text, scope: p.scope, from: p.from, until: p.until, scopes: scopes)
        p.rev += 1
        p.updatedAt = Self.now
        memory.policies[i] = p
        return p
    }

    private func checkPolicy(text: String, scope: String, from: UInt64?, until: UInt64?, scopes: [String]) throws(Refusal) {
        if let problem = PaneSession.policyTextProblem(text) { throw Refusal(.badRequest, problem) }
        guard scope == "all" || scopes.contains(scope) else {
            throw Refusal(.badRequest, "scope must be all or a workspace other than the overall one")
        }
        if let until, until <= Self.now { throw Refusal(.badRequest, "until is in the past") }
        if let from, let until, until <= from { throw Refusal(.badRequest, "until must come after from") }
    }

    // MARK: Today

    /// `update-plan` on the newest `resulted` plan: one correction per change.
    mutating func updatePlan(
        _ id: String, rev: UInt64, action: PlanAction, order: [String], row: String?, text: String?
    ) throws(Refusal) -> Plan {
        guard let i = plans.plans.firstIndex(where: { $0.delivery == .resulted }), plans.plans[i].id == id else {
            throw Refusal(.conflict, "\(id) is not the newest plan")
        }
        guard plans.plans[i].rev == rev else { throw Refusal(.conflict, "\(id) is at rev \(plans.plans[i].rev)") }
        let rows = plans.plans[i].top.compactMap { $0.work ?? $0.item }
        switch action {
        case .reorder:
            guard row == nil, text == nil else { throw Refusal(.badRequest, "reorder takes only order") }
            guard order.count == rows.count, Set(order) == Set(rows) else {
                throw Refusal(.badRequest, "order must name each row of top once")
            }
            let shown = plans.plans[i].userOrder.isEmpty ? rows : plans.plans[i].userOrder
            guard order != shown else { return plans.plans[i] }
            plans.plans[i].userOrder = order
            record(.reordered, row: nil, before: shown, after: order, reason: nil)
        case .dismiss:
            guard order.isEmpty, let row, rows.contains(row) else { throw Refusal(.badRequest, "row must be a row of top") }
            if let text, text.utf8.count > PaneSession.textLimit { throw Refusal(.badRequest, "the reason has at most 500 bytes") }
            guard !plans.plans[i].dismissed.contains(row) else { return plans.plans[i] }
            plans.plans[i].dismissed.append(row)
            record(.dismissed, row: row, reason: text)
        }
        plans.plans[i].rev += 1
        return plans.plans[i]
    }

    /// Records what the user did while a plan is current; nothing without one.
    mutating func record(
        _ kind: CorrectionKind, row: String?, before: [String] = [], after: [String] = [], reason: String?
    ) {
        guard keepsMemory, let plan = plans.plans.first(where: { $0.delivery == .resulted }) else { return }
        let inTop = row.map { r in plan.top.contains { $0.work == r || $0.item == r } } ?? true
        let workspace = row.flatMap { r in
            assistant.work.first { $0.id == r }?.workspace ?? taskItems.first { $0.id == r }?.workspace
        }
        let correction = Correction(
            id: memory.newId("x"), kind: kind, at: Self.now, plan: plan.id, row: row, inTop: inTop, workspace: workspace,
            before: before, after: after, reason: reason.flatMap { $0.isEmpty ? nil : $0 })
        memory.corrections.insert(correction, at: 0)
        memory.corrections = Array(memory.corrections.prefix(200))
    }

    /// Pin, Unpin and Later on a Work item, at every level; one that changes nothing records nothing.
    mutating func observe(_ action: WorkAction, before: WorkItem?, after: WorkItem) {
        switch action {
        case .pin where before?.pinnedAt == nil: record(.pinned, row: after.id, reason: nil)
        case .unpin where before?.pinnedAt != nil: record(.unpinned, row: after.id, reason: nil)
        case .later: record(.deferred, row: after.id, reason: nil)
        default: break
        }
    }

    /// An answer the host observes: Reject on a proposal or a hypothesis, Later on a row of the plan.
    mutating func noteAnswer(_ item: InboxItem, _ action: InboxAction, text: String?) {
        switch action {
        case .reject where [.sort, .delegate, .hypothesis].contains(item.kind):
            if let h = item.hypothesis?.hypothesis, let j = memory.hypotheses.firstIndex(where: { $0.id == h }) {
                memory.hypotheses[j].state = .rejected
                memory.hypotheses[j].expiresAt = Self.now + 90 * 86_400
            }
            record(.rejected, row: item.id, reason: text)
        case .later where plans.plans.first(where: { $0.delivery == .resulted })?.top.contains { $0.item == item.id } == true:
            record(.deferred, row: item.id, reason: text)
        default:
            break
        }
    }

    // MARK: Hypotheses

    /// `confirm` on a `hypothesis` item: a policy with its text and scope, once.
    mutating func confirm(_ id: String) throws(Refusal) -> (Policy, InboxItem) {
        guard let i = taskItems.firstIndex(where: { $0.id == id }), let proposal = taskItems[i].hypothesis else {
            throw Refusal(.badRequest, "\(id) is not a hypothesis item")
        }
        if let made = memory.policies.first(where: { $0.source == proposal.hypothesis }) { return (made, taskItems[i]) }
        guard taskItems[i].state == .open else { throw Refusal(.badRequest, "\(id) is \(taskItems[i].state.raw)") }
        let active = memory.policies.filter { !$0.isExpired(now: Self.now) }.count
        guard active < DemoPolicies.activeLimit else {
            throw Refusal(.conflict, "\(DemoPolicies.activeLimit) policies are active, the limit. Delete one first.")
        }
        let policy = Policy(id: memory.newId("y"), rev: 1, text: proposal.text, scope: proposal.scope,
                            source: proposal.hypothesis, createdAt: Self.now, updatedAt: Self.now)
        memory.policies.append(policy)
        if let j = memory.hypotheses.firstIndex(where: { $0.id == proposal.hypothesis }) {
            memory.hypotheses[j].state = .confirmed
        }
        taskItems[i].state = .answered
        taskItems[i].answer = .confirm
        taskItems[i].answeredAt = Self.now
        taskItems[i].answers = []
        return (policy, taskItems[i])
    }

    /// `forget`: a correction with the proposed hypotheses it grounds, or a hypothesis,
    /// with their Inbox items.
    mutating func forget(_ record: String) throws(Refusal) {
        if memory.corrections.contains(where: { $0.id == record }) {
            let grounded = memory.hypotheses.filter { $0.state == .proposed && $0.evidence.contains(record) }.map(\.id)
            for id in grounded { dropHypothesis(id) }
            memory.corrections.removeAll { $0.id == record }
            return
        }
        guard let h = memory.hypotheses.first(where: { $0.id == record }) else {
            throw Refusal(.badRequest, "no correction or hypothesis \(record)")
        }
        if h.state == .confirmed, let p = memory.policies.first(where: { $0.source == record }) {
            throw Refusal(.badRequest, "\(record) became policy \(p.id): delete the policy instead")
        }
        dropHypothesis(record)
    }

    private mutating func dropHypothesis(_ id: String) {
        memory.hypotheses.removeAll { $0.id == id }
        taskItems.removeAll { $0.hypothesis?.hypothesis == id }
    }

    // MARK: Fixtures

    /// What the user told the overall assistant: a policy for this week, one for a
    /// workspace, an expired one; three corrections; a hypothesis grounded on two of them,
    /// with its Inbox item. The plan's rows cite them, and one correction that was forgotten.
    func remembered(
        ids n: Int, item: String, row: String, dismissed: String, workspace: String, hypothesis text: String, reason: String
    ) -> DemoWork {
        let now = Self.now
        let id = { (prefix: String, k: Int) in DemoPolicies.id(prefix, n + k) }
        var w = self
        w.memory.policies = [
            Policy(id: id("y", 1), rev: 1, text: "This week, recruiting comes first.", scope: "all",
                   until: UInt64(PaneSession.endOfWeek(Date()).timeIntervalSince1970), source: "user",
                   createdAt: now - 86_400, updatedAt: now - 86_400),
            Policy(id: id("y", 2), rev: 1, text: "Keep mornings free for deep work.", scope: workspace, source: "user",
                   createdAt: now - 5 * 86_400, updatedAt: now - 5 * 86_400),
            Policy(id: id("y", 3), rev: 1, text: "Finish the tax return before anything personal.", scope: "all",
                   until: now - 2 * 86_400, source: "user", createdAt: now - 20 * 86_400, updatedAt: now - 20 * 86_400),
        ]
        let plan = w.plans.plans.first?.id ?? DemoPlans.id(1)
        w.memory.corrections = [
            Correction(id: id("x", 3), kind: .dismissed, at: now - 3600, plan: plan, row: dismissed, inTop: true,
                       workspace: workspace, reason: reason),
            Correction(id: id("x", 2), kind: .deferred, at: now - 86_400, plan: plan, row: dismissed, inTop: true,
                       workspace: workspace),
            Correction(id: id("x", 1), kind: .pinned, at: now - 2 * 86_400, plan: plan, row: row, inTop: false,
                       workspace: workspace),
        ]
        let hypothesis = Hypothesis(
            id: id("h", 1), text: text, scope: workspace, evidence: [id("x", 3), id("x", 2)], confidence: .low,
            state: .proposed, plan: plan, expiresAt: now + 14 * 86_400, at: now - 1800)
        w.memory.hypotheses = [hypothesis]
        w.memory.next = n + 0x10
        w.taskItems.insert(InboxItem(
            id: item, kind: .hypothesis, state: .open, question: "Hypothesis: \(text)",
            workspace: DemoPlans.overall, title: "Overall assistant", createdAt: now - 1800,
            hypothesis: HypothesisProposal(hypothesis: hypothesis.id, text: text, scope: workspace,
                                           evidence: hypothesis.evidence, confidence: .low),
            answers: [.confirm, .reject, .later]), at: 0)
        if let r = w.plans.plans.first?.top.firstIndex(where: { $0.work == row }) {
            w.plans.plans[0].top[r].basis += [
                Basis(kind: .policy, text: "Mornings stay free for deep work", source: id("y", 2)),
                Basis(kind: .hypothesis, text: "Billing waits on finance", source: hypothesis.id),
                Basis(kind: .correction, text: "You moved it down earlier", source: id("x", 3)),
                Basis(kind: .correction, text: "You moved it before", source: id("x", 0xff)),
            ]
        }
        return w
    }

    /// The UI tests' `-demo-policies`: `-demo-plans` with what the user told the assistant.
    /// w-0000b001's row cites policy y-0000a102, hypothesis h-0000a101, correction x-0000a103
    /// and the forgotten x-0000a1ff; the hypothesis's item is i-0000f003.
    static var testsRemembered: DemoWork {
        testsPlanned.remembered(
            ids: 0x100, item: "i-0000f003", row: "w-0000b001", dismissed: "w-0000b001", workspace: "work",
            hypothesis: "Billing work waits on finance's numbers. Rank it lower until they arrive.",
            reason: "Waiting on finance's numbers.")
    }

    static var showcaseRemembered: DemoWork {
        showcasePlanned.remembered(
            ids: 0x500, item: "i-5c00f003", row: "w-5c00b001", dismissed: "w-5c00b002", workspace: "work",
            hypothesis: "Pricing work waits on the sales numbers. Rank it lower until they arrive.",
            reason: "Sales sends the numbers on Friday.")
    }
}
