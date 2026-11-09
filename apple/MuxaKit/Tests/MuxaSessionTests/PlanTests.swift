import Foundation
import MuxaProtocol
import Testing

@testable import MuxaSession

/// A bridge with the overall assistant (`work` level 9, ADR 0047), as the host answers:
/// plans, notes, pins and due dates, and the `sort` and `delegate` items.
final class PlanBridge: AssistantBridge, @unchecked Sendable {
    var plans: [Plan] = [PlanTests.plan("p-00000001", .resulted, top: [
        PlanRow(work: "w-00000001", why: "Due Friday.", next: "Fill in the table.",
                minutes: Minutes(low: 30, high: 60, estimated: 45)),
    ], unanswered: ["q-00000009"])]
    var assistant = AssistantInfo(overall: "assistant", delegates: [
        DelegateInfo(workspace: "work", share: .summaries, overallRequests: .review),
    ])
    var items: [InboxItem] = [PlanTests.delegateItem]
    /// The live notes' hash: `apply-notes` with another base is `conflict`.
    var liveNotes = "base-1"
    /// `plan-now` is refused with this.
    var planRefusal: ErrorBody?
    /// An approved `delegate` is refused with this, after its `work-saved`.
    var approveRefusal: ErrorBody?
    private let sentLock = NSLock()
    private var _planRequests: [Request] = []
    /// The level 9 requests the session sent, in order.
    var planRequests: [Request] { sentLock.withLock { _planRequests } }

    override func send(_ line: Data) async throws {
        let req = try Wire.decodeRequest(line)
        switch req {
        case .listInbox(let id):
            emit(.inboxList(inbox: id, entries: [], items: items, work: work,
                            captures: captures.filter { $0.state == .received }, requests: requests, plans: plans,
                            assistant: assistant))
            emit(.result(id: id, error: nil, sessions: nil))
        case .planNow(let id, let minutes):
            note(req)
            if let planRefusal { return emit(.result(id: id, error: planRefusal, sessions: nil)) }
            var made = PlanTests.plan(String(format: "p-%08x", plans.count + 1), .pending)
            made.minutes = minutes
            plans.insert(made, at: 0)
            emit(.planSaved(plan: id, entry: made))
            emit(.result(id: id, error: nil, sessions: nil))
        case .applyNotes(let id, _, _, let base):
            note(req)
            guard base == liveNotes else {
                return emit(.result(id: id, error: ErrorBody(code: .conflict, message: "NOTES.md changed"), sessions: nil))
            }
            liveNotes = "base-2"
            emit(.workSaved(work: id, entry: work[0], request: requests.first))
            emit(.result(id: id, error: nil, sessions: nil))
        case .updateWork(let id, let item, let rev, let action, _, _, _, let due) where action == .pin || action == .unpin || due != nil:
            note(req)
            guard let i = work.firstIndex(where: { $0.id == item }), work[i].rev == rev else {
                return emit(.result(id: id, error: ErrorBody(code: .conflict, message: "stale"), sessions: nil))
            }
            work[i].rev += 1
            if action == .pin { work[i].pinnedAt = 7 }
            if action == .unpin { work[i].pinnedAt = nil }
            if let due { work[i].due = due }
            emit(.workSaved(work: id, entry: work[i]))
            emit(.result(id: id, error: nil, sessions: nil))
        case .updateInboxEntry(let id, let entry, let action, _, _, _, _):
            note(req)
            guard let i = items.firstIndex(where: { $0.id == entry }) else {
                return emit(.result(id: id, error: ErrorBody(code: .badRequest, message: "no item"), sessions: nil))
            }
            if action == .approve, let d = items[i].delegate, let w = work.firstIndex(where: { $0.id == d.work }) {
                let made = Self.request("q-00000002", work: d.work, scope: d.scope)
                if let approveRefusal {
                    var failed = made
                    failed.delivery = .failed
                    emit(.workSaved(work: id, entry: work[w], request: failed))
                    emit(.inboxSaved(inbox: id, entry: nil, item: items[i]))
                    return emit(.result(id: id, error: approveRefusal, sessions: nil))
                }
                if !requests.contains(where: { $0.id == made.id }) {
                    requests.append(made)
                    work[w].requests.append(made.id)
                    work[w].rev += 1
                }
                emit(.workSaved(work: id, entry: work[w], request: made))
            }
            items[i].state = .answered
            items[i].answer = action
            items[i].answers = []
            emit(.inboxSaved(inbox: id, entry: nil, item: items[i]))
            emit(.result(id: id, error: nil, sessions: nil))
        default:
            try await super.send(line)
        }
    }

    private func note(_ req: Request) { sentLock.withLock { _planRequests.append(req) } }
}

@MainActor
struct PlanTests {
    nonisolated static func plan(
        _ id: String, _ delivery: Delivery, top: [PlanRow] = [], unanswered: [String] = []
    ) -> Plan {
        Plan(id: id, rev: 1, at: 5, inputsAt: 5, delivery: delivery, deliveryAt: 5, top: top, unanswered: unanswered)
    }

    nonisolated static let delegateItem = InboxItem(
        id: "i-0000000d", kind: .delegate, state: .open, question: "Ask work's assistant to compare offers",
        workspace: "assistant", title: "Plan the migration", createdAt: 5,
        delegate: DelegateProposal(work: "w-00000001", workspace: "work", scope: .compare,
                                   purpose: "Prices of the two offers", reason: "Due Friday."),
        answers: [.approve, .reject, .later])

    static let notes = Notes(text: "# Work\n\n- Invoices come from the old sequence.\n", base: "base-1")

    func connected(_ bridge: PlanBridge, level: String = "work:9") async throws -> PaneSession {
        bridge.capabilities = ["task-records", level]
        bridge.workspaces = [WorkspaceInfo(name: "assistant"), WorkspaceInfo(name: "work")]
        let session = PaneSession()
        try await session.connect {
            bridge.start()
            return bridge
        }
        await session.loadTasks()
        return session
    }

    @Test func theInboxKeepsPlansAndLeavesTheOverallWorkspaceOutOfPickers() async throws {
        let session = try await connected(PlanBridge())
        #expect(session.plans.map(\.id) == ["p-00000001"])
        #expect(session.currentPlan?.top.first?.work == "w-00000001")
        #expect(session.planRow(forWork: "w-00000001")?.why == "Due Friday.")
        #expect(session.assistantInfo?.overall == "assistant")
        #expect(session.workspaces.map(\.name) == ["assistant", "work"])
        #expect(session.pickableWorkspaces.map(\.name) == ["work"])
        var request = AssistantBridge.request("q-00000009", work: "w-00000001", scope: .research)
        #expect(session.isUnanswered(request))
        request.id = "q-00000008"
        #expect(!session.isUnanswered(request))
    }

    @Test func aLevel8HostShowsNoPlans() async throws {
        let bridge = PlanBridge()
        let session = try await connected(bridge, level: "work:8")
        #expect(session.plans.isEmpty)
        #expect(session.assistantInfo == nil)
        #expect(session.pickableWorkspaces.map(\.name) == ["assistant", "work"])
        let unsupported = PaneSession.Failure.bridge(ErrorBody(code: .unsupported, message: PaneSession.plansUnsupported))
        await #expect(throws: unsupported) { try await session.planNow() }
        await #expect(throws: unsupported) { try await session.update(session.workList[0], .pin) }
        await #expect(throws: unsupported) {
            try await session.update(session.workList[0], .edit, due: Due(at: 9, timezone: "Asia/Tokyo"))
        }
        let request = AssistantBridge.request("q-00000001", work: "w-00000001", scope: .research)
        await #expect(throws: unsupported) {
            try await session.applyNotes(of: request, result: AssistResult(version: 1, at: 5, summary: "S", notes: Self.notes))
        }
        #expect(bridge.planRequests.isEmpty)
    }

    @Test func planNowSendsTheMinutesAndKeepsThePendingPlanFirst() async throws {
        let bridge = PlanBridge()
        let session = try await connected(bridge)
        let made = try await session.planNow(minutes: 45)
        #expect(made.delivery == .pending)
        #expect(made.minutes == 45)
        #expect(session.plans.map(\.id) == ["p-00000002", "p-00000001"])
        #expect(session.isPlanning)
        #expect(session.currentPlan?.id == "p-00000001", "Today keeps the plan with rows")
        #expect(bridge.planRequests.contains { if case .planNow(_, 45) = $0 { true } else { false } })
        #expect(session.savedPlans.isEmpty)
    }

    @Test func planNowKeepsTheHostsRefusal() async throws {
        let bridge = PlanBridge()
        let busy = ErrorBody(code: .conflict, message: "plan p-00000001 is being made")
        bridge.planRefusal = busy
        let session = try await connected(bridge)
        await #expect(throws: PaneSession.Failure.bridge(busy)) { try await session.planNow() }
    }

    @Test func applyNotesSendsTheResultsVersionAndBase() async throws {
        let bridge = PlanBridge()
        let session = try await connected(bridge)
        var request = AssistantBridge.request("q-00000001", work: "w-00000001", scope: .research)
        request.version = 3
        let result = AssistResult(version: 3, at: 5, summary: "S", notes: Self.notes)
        #expect(!session.notesApplied(request, result))
        try await session.applyNotes(of: request, result: result)
        #expect(session.notesApplied(request, result))
        #expect(bridge.planRequests.contains {
            if case .applyNotes(_, "q-00000001", 3, "base-1") = $0 { true } else { false }
        })
        #expect(session.savedWork.isEmpty && session.savedRequests.isEmpty)
        // The live file changed: the host refuses, and the message says to ask again.
        await #expect(throws: PaneSession.Failure.bridge(ErrorBody(code: .conflict, message: PaneSession.notesConflict))) {
            try await session.applyNotes(of: request, result: result)
        }
    }

    @Test func pinUnpinAndDueSendUpdateWork() async throws {
        let bridge = PlanBridge()
        let session = try await connected(bridge)
        let pinned = try await session.update(session.workList[0], .pin)
        #expect(pinned.pinnedAt == 7)
        let unpinned = try await session.update(session.workList[0], .unpin)
        #expect(unpinned.pinnedAt == nil)
        let due = Due(at: 1_762_930_800, timezone: "Asia/Tokyo")
        let dated = try await session.update(session.workList[0], .edit, due: due)
        #expect(dated.due == due)
        let actions = bridge.planRequests.compactMap {
            if case .updateWork(_, "w-00000001", let rev, let action, nil, nil, nil, let due) = $0 { (rev, action, due) } else { nil }
        }
        #expect(actions.map(\.0) == [1, 2, 3], "each sends the rev it saw")
        #expect(actions.map(\.1) == [.pin, .unpin, .edit])
        #expect(actions.map(\.2) == [nil, nil, due])
    }

    @Test func approvingADelegateMakesItsRequestAndListsIt() async throws {
        let bridge = PlanBridge()
        let session = try await connected(bridge)
        let item = try #require(session.inboxItems.first)
        let saved = try await session.answer(item, .approve)
        #expect(saved.state == .answered)
        #expect(saved.answer == .approve)
        #expect(session.requests(of: session.workList[0]).map(\.id) == ["q-00000002"])
        #expect(session.savedWork.isEmpty && session.savedRequests.isEmpty && session.savedCaptures.isEmpty)
        #expect(bridge.planRequests.contains {
            if case .updateInboxEntry(_, "i-0000000d", .approve, nil, nil, nil, nil) = $0 { true } else { false }
        })
    }

    @Test func aRefusedApprovalKeepsTheHostsReason() async throws {
        let bridge = PlanBridge()
        bridge.approveRefusal = AssistantBridge.budget
        let session = try await connected(bridge)
        let item = try #require(session.inboxItems.first)
        await #expect(throws: PaneSession.Failure.bridge(AssistantBridge.budget)) {
            try await session.answer(item, .approve)
        }
        #expect(session.savedWork.isEmpty && session.savedRequests.isEmpty)
        #expect(session.inboxItems.first?.state == .open, "the item stays open")
    }

    @Test func rejectAnswersTheItemOnly() async throws {
        let bridge = PlanBridge()
        let session = try await connected(bridge)
        let item = try #require(session.inboxItems.first)
        let saved = try await session.answer(item, .reject)
        #expect(saved.answer == .reject)
        #expect(bridge.requests.isEmpty, "no request made")
    }
}
