import Foundation
import MuxaProtocol
import Testing

@testable import MuxaSession

/// A bridge with policies, corrections and hypotheses (`work` level 11, ADR 0050), as
/// the host answers them: one active and one expired policy, a proposed hypothesis with
/// its Inbox item, and two corrections, one of which grounds the hypothesis.
final class PolicyBridge: PlanBridge, @unchecked Sendable {
    var policies = [PolicyTests.policy("y-00000001", "Recruiting comes first this week.")]
    var hypotheses = [PolicyTests.hypothesis]
    var corrections = [
        Correction(id: "x-00000002", kind: .dismissed, at: 20, plan: "p-00000001", row: "w-00000001", inTop: true,
                   workspace: "work", reason: "Waiting on finance."),
        Correction(id: "x-00000001", kind: .deferred, at: 10, plan: "p-00000001", row: "w-00000001", inTop: true),
    ]
    private let memoryLock = NSLock()
    private var _memoryRequests: [Request] = []
    /// The level 11 requests the session sent, in order.
    var memoryRequests: [Request] { memoryLock.withLock { _memoryRequests } }

    override init() {
        super.init()
        policies.append(PolicyTests.policy("y-00000002", "Sales first in October.", until: 40))
        items.append(PolicyTests.hypothesisItem)
        plans[0].top.append(PlanRow(item: "i-0000000d", why: "Approve it.", next: "Open it.",
                                    minutes: Minutes(low: 1, high: 5, estimated: 2)))
    }

    override func send(_ line: Data) async throws {
        let req = try Wire.decodeRequest(line)
        switch req {
        case .listInbox(let id):
            emit(.inboxList(inbox: id, entries: [], items: items, work: work,
                            captures: captures.filter { $0.state == .received }, requests: requests, plans: plans,
                            assistant: assistant, policies: policies, hypotheses: hypotheses, corrections: corrections))
            emit(.result(id: id, error: nil, sessions: nil))
        case .addPolicy(let id, let text, let scope, let from, let until):
            note(req)
            let made = Policy(id: String(format: "y-%08x", policies.count + 1), rev: 1, text: text, scope: scope, from: from,
                              until: until, source: "user", createdAt: 30, updatedAt: 30)
            policies.insert(made, at: 0)
            emit(.policySaved(policy: id, entry: made))
            emit(.result(id: id, error: nil, sessions: nil))
        case .updatePolicy(let id, let policy, let rev, let action, let text, let scope, _, let until):
            note(req)
            guard let i = policies.firstIndex(where: { $0.id == policy }), policies[i].rev == rev else {
                return emit(.result(id: id, error: ErrorBody(code: .conflict, message: "stale rev"), sessions: nil))
            }
            if action == .delete {
                policies.remove(at: i)
                return emit(.result(id: id, error: nil, sessions: nil))
            }
            policies[i].rev += 1
            if let text { policies[i].text = text }
            if let scope { policies[i].scope = scope }
            if let until { policies[i].until = until }
            emit(.policySaved(policy: id, entry: policies[i]))
            emit(.result(id: id, error: nil, sessions: nil))
        case .updatePlan(let id, let plan, let rev, let action, let order, let row, let text):
            note(req)
            guard plans[0].id == plan, plans[0].rev == rev else {
                return emit(.result(id: id, error: ErrorBody(code: .conflict, message: "not the newest plan"), sessions: nil))
            }
            plans[0].rev += 1
            let kind: CorrectionKind = action == .reorder ? .reordered : .dismissed
            if action == .reorder { plans[0].userOrder = order }
            if action == .dismiss, let row { plans[0].dismissed.append(row) }
            corrections.insert(Correction(id: String(format: "x-%08x", corrections.count + 1), kind: kind, at: 40, plan: plan,
                                          row: row, inTop: true, reason: text), at: 0)
            emit(.planSaved(plan: id, entry: plans[0]))
            emit(.result(id: id, error: nil, sessions: nil))
        case .forget(let id, let record):
            note(req)
            if corrections.contains(where: { $0.id == record }) {
                corrections.removeAll { $0.id == record }
                // The hypotheses it grounds go with it, and their items.
                let gone = hypotheses.filter { $0.evidence.contains(record) }.map(\.id)
                hypotheses.removeAll { gone.contains($0.id) }
                items.removeAll { $0.hypothesis.map { gone.contains($0.hypothesis) } ?? false }
            } else if hypotheses.contains(where: { $0.id == record }) {
                hypotheses.removeAll { $0.id == record }
                items.removeAll { $0.hypothesis?.hypothesis == record }
            } else {
                return emit(.result(id: id, error: ErrorBody(code: .badRequest, message: "no record \(record)"), sessions: nil))
            }
            emit(.result(id: id, error: nil, sessions: nil))
        case .updateInboxEntry(let id, let entry, let action, _, let text, _, _, _)
            where items.contains { $0.id == entry && $0.kind == .hypothesis }:
            note(req)
            guard let i = items.firstIndex(where: { $0.id == entry }), let h = items[i].hypothesis,
                  let j = hypotheses.firstIndex(where: { $0.id == h.hypothesis })
            else { return emit(.result(id: id, error: ErrorBody(code: .badRequest, message: "no item"), sessions: nil)) }
            if action == .confirm {
                let made = Policy(id: "y-0000000c", rev: 1, text: h.text, scope: h.scope, source: h.hypothesis,
                                  createdAt: 50, updatedAt: 50)
                policies.insert(made, at: 0)
                hypotheses.remove(at: j)
                emit(.policySaved(policy: id, entry: made))
            } else if action == .reject {
                hypotheses[j].state = .rejected
                corrections.insert(Correction(id: "x-0000000r", kind: .rejected, at: 50, plan: "p-00000001",
                                              row: entry, inTop: false, reason: text), at: 0)
            }
            items[i].state = .answered
            items[i].answer = action
            items[i].answers = []
            emit(.inboxSaved(inbox: id, entry: nil, item: items[i]))
            emit(.result(id: id, error: nil, sessions: nil))
        case .updateInboxEntry(_, _, .reject, _, _, _, _, _):
            note(req)
            try await super.send(line)
        default:
            try await super.send(line)
        }
    }

    private func note(_ req: Request) { memoryLock.withLock { _memoryRequests.append(req) } }
}

@MainActor
struct PolicyTests {
    nonisolated static func policy(_ id: String, _ text: String, until: UInt64? = nil) -> Policy {
        Policy(id: id, rev: 1, text: text, scope: "all", until: until, source: "user", createdAt: 5, updatedAt: 5)
    }

    nonisolated static let hypothesis = Hypothesis(
        id: "h-00000001", text: "Billing work waits on finance.", scope: "work", evidence: ["x-00000002"],
        confidence: .low, state: .proposed, plan: "p-00000001", expiresAt: 9_999, at: 20)

    nonisolated static let hypothesisItem = InboxItem(
        id: "i-0000000h", kind: .hypothesis, state: .open, question: "Billing work waits on finance?",
        workspace: "assistant", title: "Overall assistant", createdAt: 20,
        hypothesis: HypothesisProposal(hypothesis: "h-00000001", text: "Billing work waits on finance.", scope: "work",
                                       evidence: ["x-00000002"], confidence: .low),
        answers: [.confirm, .reject, .later])

    func connected(_ bridge: PolicyBridge, level: String = "work:11") async throws -> PaneSession {
        try await PlanTests().connected(bridge, level: level)
    }

    @Test func theInboxListKeepsPoliciesHypothesesAndCorrections() async throws {
        let session = try await connected(PolicyBridge())
        #expect(session.canUsePolicies)
        #expect(session.activePolicies(now: 50).map(\.id) == ["y-00000001"])
        #expect(session.expiredPolicies(now: 50).map(\.id) == ["y-00000002"])
        #expect(session.hypotheses.map(\.id) == ["h-00000001"])
        #expect(session.corrections.map(\.id) == ["x-00000002", "x-00000001"])
        let hypothesis = try #require(session.hypotheses.first)
        #expect(session.item(for: hypothesis)?.id == "i-0000000h")
    }

    @Test func aLevel10HostKeepsNoMemoryAndIsSentNothing() async throws {
        let bridge = PolicyBridge()
        let session = try await connected(bridge, level: "work:10")
        #expect(session.policies.isEmpty && session.hypotheses.isEmpty && session.corrections.isEmpty)
        #expect(session.cited(Basis(kind: .policy, text: "P", source: "y-00000001")) == nil, "shown as the plan wrote it")
        let unsupported = PaneSession.Failure.bridge(ErrorBody(code: .unsupported, message: PaneSession.policiesUnsupported))
        await #expect(throws: unsupported) { try await session.addPolicy(text: "Recruiting first.", scope: "all") }
        await #expect(throws: unsupported) { try await session.reorderToday(["w-00000001"]) }
        await #expect(throws: unsupported) { try await session.dismissFromToday("w-00000001") }
        await #expect(throws: unsupported) { try await session.forget("x-00000001") }
        let item = try #require(session.inboxItems.first { $0.kind == .delegate })
        _ = try await session.reject(item, reason: "Not now.")
        #expect(bridge.memoryRequests.allSatisfy {
            if case .updateInboxEntry(_, _, .reject, _, nil, _, _, _) = $0 { true } else { false }
        }, "a level 10 host gets no reason")
    }

    @Test func addEditAndDeleteAPolicy() async throws {
        let bridge = PolicyBridge()
        let session = try await connected(bridge)
        let made = try await session.addPolicy(text: "Recruiting comes first.", scope: "work", until: 900)
        #expect(made.scope == "work")
        #expect(session.activePolicies(now: 50).contains { $0.id == made.id })
        #expect(session.savedPolicies.isEmpty)
        let edited = try await session.editPolicy(made, text: "Recruiting comes first, then sales.")
        #expect(edited.rev == 2)
        #expect(session.policies.first { $0.id == made.id }?.text == "Recruiting comes first, then sales.")
        #expect(bridge.memoryRequests.contains {
            if case .updatePolicy(_, made.id, 1, .edit, "Recruiting comes first, then sales.", nil, nil, nil) = $0 {
                true
            } else { false }
        })
        // The first version is stale now: the host refuses, and the list has the host's version.
        await #expect(throws: PaneSession.Failure.bridge(ErrorBody(code: .conflict, message: "stale rev"))) {
            try await session.editPolicy(made, text: "Old")
        }
        #expect(session.policies.first { $0.id == made.id }?.rev == 2)
        try await session.deletePolicy(edited)
        #expect(!session.policies.contains { $0.id == made.id })
    }

    @Test func reorderSendsEveryRowAndTodayFollowsTheUsersOrder() async throws {
        let bridge = PolicyBridge()
        let session = try await connected(bridge)
        let plan = try #require(session.currentPlan)
        #expect(session.todayRows(of: plan).compactMap(\.rowId) == ["w-00000001", "i-0000000d"])
        let saved = try await session.reorderToday(["i-0000000d", "w-00000001"])
        #expect(saved.userOrder == ["i-0000000d", "w-00000001"])
        #expect(bridge.memoryRequests.contains {
            if case .updatePlan(_, "p-00000001", 1, .reorder, ["i-0000000d", "w-00000001"], nil, nil) = $0 { true } else { false }
        })
        let current = try #require(session.currentPlan)
        #expect(session.todayRows(of: current).compactMap(\.rowId) == ["i-0000000d", "w-00000001"])
        #expect(current.top.compactMap(\.rowId) == ["w-00000001", "i-0000000d"], "top keeps the agent's order")
        #expect(session.corrections.first?.kind == .reordered, "the host's correction is listed")
        #expect(session.savedPlans.isEmpty)
    }

    @Test func notTodaySendsTheReasonOnlyWhenGiven() async throws {
        let bridge = PolicyBridge()
        let session = try await connected(bridge)
        try await session.dismissFromToday("w-00000001", reason: "  ")
        let plan = try #require(session.currentPlan)
        let row = try #require(plan.top.first)
        #expect(session.isDismissed(row, in: plan))
        try await session.dismissFromToday("i-0000000d", reason: " Waiting on finance. ")
        let dismissals = bridge.memoryRequests.compactMap {
            if case .updatePlan(_, _, _, .dismiss, [], let row, let text) = $0 { (row, text) } else { nil }
        }
        #expect(dismissals.map(\.0) == ["w-00000001", "i-0000000d"])
        #expect(dismissals.map(\.1) == [nil, "Waiting on finance."])
    }

    @Test func aChangedTodaySaysToTryAgain() async throws {
        let bridge = PolicyBridge()
        let session = try await connected(bridge)
        bridge.plans[0].rev = 7
        await #expect(throws: PaneSession.Failure.bridge(ErrorBody(code: .conflict, message: PaneSession.todayConflict))) {
            try await session.reorderToday(["i-0000000d", "w-00000001"])
        }
        #expect(session.currentPlan?.rev == 7, "the list has the host's plan")
    }

    @Test func confirmMakesAPolicyOfTheHypothesis() async throws {
        let bridge = PolicyBridge()
        let session = try await connected(bridge)
        let item = try #require(session.inboxItems.first { $0.kind == .hypothesis })
        let saved = try await session.confirm(item)
        #expect(saved.answer == .confirm)
        let policy = try #require(session.policies.first { $0.source == "h-00000001" })
        #expect(policy.text == "Billing work waits on finance.")
        #expect(policy.scope == "work")
        #expect(session.hypotheses.isEmpty)
        #expect(session.savedPolicies.isEmpty)
    }

    @Test func rejectSendsTheReasonAndListsTheCorrection() async throws {
        let bridge = PolicyBridge()
        let session = try await connected(bridge)
        let item = try #require(session.inboxItems.first { $0.kind == .hypothesis })
        _ = try await session.reject(item, reason: "It is the budget, not finance.")
        #expect(bridge.memoryRequests.contains {
            if case .updateInboxEntry(_, "i-0000000h", .reject, nil, "It is the budget, not finance.", nil, nil, nil) = $0 {
                true
            } else { false }
        })
        #expect(session.hypotheses.first?.state == .rejected)
        #expect(session.corrections.first?.reason == "It is the budget, not finance.")
        let delegate = try #require(session.inboxItems.first { $0.kind == .delegate })
        _ = try await session.reject(delegate, reason: "")
        #expect(bridge.memoryRequests.contains {
            if case .updateInboxEntry(_, "i-0000000d", .reject, nil, nil, nil, nil, _) = $0 { true } else { false }
        }, "an empty reason is none")
    }

    @Test func forgettingACorrectionRemovesTheHypothesesItGrounds() async throws {
        let bridge = PolicyBridge()
        let session = try await connected(bridge)
        let cites = Basis(kind: .correction, text: "You moved it down.", source: "x-00000002")
        guard case .correction(let correction) = session.cited(cites) else { Issue.record("no correction"); return }
        #expect(correction.reason == "Waiting on finance.")
        try await session.forget("x-00000002")
        #expect(session.corrections.map(\.id) == ["x-00000001"])
        #expect(session.hypotheses.isEmpty)
        #expect(!session.inboxItems.contains { $0.kind == .hypothesis }, "its item is gone too")
        #expect(session.cited(cites) == .forgotten)
        #expect(session.cited(Basis(kind: .policy, text: "P", source: "y-00000001")) == .policy(session.policies[0]))
        #expect(session.cited(Basis(kind: .due, text: "Friday")) == nil)
        await #expect(throws: PaneSession.Failure.bridge(ErrorBody(code: .badRequest, message: "no record x-00000002"))) {
            try await session.forget("x-00000002")
        }
    }

    @Test func aHypothesisItemOffersConfirmFirst() {
        let answers = ItemAnswers(Self.hypothesisItem)
        #expect(answers.decisions == [.confirm, .reject])
        #expect(answers.main == .confirm)
        #expect(answers.later)
        #expect(InboxAction.confirm.title(for: .hypothesis) == "Confirm")
    }

    @Test func thisWeekEndsOnSundayNight() throws {
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = try #require(TimeZone(identifier: "Asia/Tokyo"))
        let wednesday = try #require(calendar.date(from: DateComponents(year: 2026, month: 11, day: 11, hour: 9)))
        let end = PaneSession.endOfWeek(wednesday, calendar: calendar)
        #expect(calendar.dateComponents([.year, .month, .day, .hour, .minute, .second], from: end)
            == DateComponents(year: 2026, month: 11, day: 15, hour: 23, minute: 59, second: 59))
        let sunday = try #require(calendar.date(from: DateComponents(year: 2026, month: 11, day: 15, hour: 9)))
        #expect(calendar.component(.day, from: PaneSession.endOfWeek(sunday, calendar: calendar)) == 15)
    }

    @Test func aPolicyIsOneParagraphOfAtMost500Bytes() {
        #expect(PaneSession.policyTextProblem("Recruiting first.") == nil)
        #expect(PaneSession.policyTextProblem("  ") != nil)
        #expect(PaneSession.policyTextProblem("One\nTwo") != nil)
        #expect(PaneSession.policyTextProblem(String(repeating: "あ", count: 167)) != nil, "501 bytes")
        #expect(PaneSession.policyTextProblem(String(repeating: "a", count: 500)) == nil)
    }
}
