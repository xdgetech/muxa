import Foundation
import MuxaProtocol
import Testing

@testable import MuxaSession

/// A bridge with staff and hand-offs (`work` level 12, ADR 0051), as the host answers:
/// the staff in `inbox-list`, and a `handoff` item whose approve starts the next step once.
final class StaffBridge: PlanBridge, @unchecked Sendable {
    /// Approve on a hand-off is refused with this, and the item stays open.
    var handoffRefusal: ErrorBody?

    override init() {
        super.init()
        assistant = AssistantInfo(overall: "assistant", delegates: [
            DelegateInfo(workspace: "work", share: .summaries, overallRequests: .review, staff: StaffTests.staff),
            DelegateInfo(workspace: "home", share: .outline, overallRequests: .none, staff: [
                StaffMember(worker: "home-helper", description: "Runs the house.", lead: true),
            ]),
        ])
        work[0].requests = ["q-00000001"]
        requests = [StaffTests.chained]
        items = [StaffTests.handoffItem]
    }

    override func send(_ line: Data) async throws {
        let req = try Wire.decodeRequest(line)
        guard case .updateInboxEntry(let id, let entry, let action, _, _, _, _) = req,
              let i = items.firstIndex(where: { $0.id == entry }), let h = items[i].handoff
        else { return try await super.send(line) }
        if action == .approve {
            if let handoffRefusal { return emit(.result(id: id, error: handoffRefusal, sessions: nil)) }
            guard let r = requests.firstIndex(where: { $0.id == h.request }) else { return }
            if !requests[r].steps.contains(where: { $0.step == h.step + 1 }) {
                requests[r].steps.append(RequestStep(
                    step: h.step + 1, worker: h.worker, by: .handoff, delivery: .pending, deliveryAt: 9, run: "t-run003"))
                requests[r].delivery = .pending
            }
            emit(.workSaved(work: id, entry: work[0], request: requests[r]))
        }
        items[i].state = .answered
        items[i].answer = action
        items[i].answers = []
        emit(.inboxSaved(inbox: id, entry: nil, item: items[i]))
        emit(.result(id: id, error: nil, sessions: nil))
    }
}

@MainActor
struct StaffTests {
    nonisolated static let staff = [
        StaffMember(worker: "chief", description: "Takes requests first.", lead: true),
        StaffMember(worker: "researcher", description: "Finds sources."),
        StaffMember(worker: "writer", description: "Drafts memos."),
    ]

    nonisolated static let notes = Notes(text: "# Researcher\n", base: "base-1")

    /// Version 1 of q-00000001: chief handed it to researcher, who asks for writer.
    nonisolated static var chained: AssistantRequest {
        var r = AssistantBridge.request("q-00000001", work: "w-00000001", scope: .research, results: [
            AssistResult(version: 1, at: 8, summary: "Researched.", notes: notes, worker: "researcher", step: 2),
            AssistResult(version: 1, at: 7, summary: "Chief's view.", notes: notes, worker: "chief", step: 1),
        ])
        r.delivery = .resulted
        r.steps = [
            RequestStep(step: 1, worker: "chief", by: .lead, delivery: .resulted, deliveryAt: 7, run: "t-run001",
                        handoff: Handoff(worker: "researcher", purpose: "Find sources.")),
            RequestStep(step: 2, worker: "researcher", by: .handoff, delivery: .resulted, deliveryAt: 8, run: "t-run002",
                        handoff: Handoff(worker: "writer", purpose: "Draft the memo.")),
        ]
        return r
    }

    nonisolated static let handoffItem = InboxItem(
        id: "i-0000000h", kind: .handoff, state: .open, question: "researcher asks writer to draft the memo.",
        workspace: "work", title: "Plan the migration", createdAt: 8,
        handoff: HandoffProposal(request: "q-00000001", version: 1, step: 2, work: "w-00000001", from: "researcher",
                                 worker: "writer", purpose: "Draft the memo."),
        answers: [.approve, .reject, .later])

    func connected(_ bridge: StaffBridge, level: String = "work:12") async throws -> PaneSession {
        bridge.capabilities = ["task-records", level]
        bridge.workspaces = [WorkspaceInfo(name: "assistant"), WorkspaceInfo(name: "work"), WorkspaceInfo(name: "home")]
        let session = PaneSession()
        try await session.connect {
            bridge.start()
            return bridge
        }
        await session.loadTasks()
        return session
    }

    @Test func theStaffAndTheChainAreListedAtLevel12() async throws {
        let session = try await connected(StaffBridge())
        #expect(session.staff(of: "work").map(\.worker) == ["chief", "researcher", "writer"])
        #expect(session.lead(of: "work")?.worker == "chief")
        #expect(session.staff(of: "home").count == 1)
        #expect(session.staff(of: nil).isEmpty)
        let request = try #require(session.requests(of: session.workList[0]).first)
        #expect(request.chain == ["chief", "researcher", "writer"])
        #expect(request.waitingHandoff?.step.step == 2)
        #expect(request.result(ofStep: 1)?.summary == "Chief's view.")
        #expect(request.result(ofStep: 2)?.summary == "Researched.")
        #expect(session.handoffItem(for: request, step: 2)?.id == "i-0000000h")
    }

    @Test func aLevel11HostHasNoStaffAndSendsNoWorker() async throws {
        let bridge = StaffBridge()
        let session = try await connected(bridge, level: "work:11")
        #expect(session.staff(of: "work").isEmpty)
        #expect(session.lead(of: "work") == nil)
        let unsupported = PaneSession.Failure.bridge(ErrorBody(code: .unsupported, message: PaneSession.staffUnsupported))
        await #expect(throws: unsupported) {
            try await session.ask(session.workList[0], scope: .research, worker: "researcher")
        }
        await #expect(throws: unsupported) {
            try await session.capture(clientId: "k1", text: "Look", workspace: "work",
                                      assist: AssistSpec(scope: .research, worker: "researcher"))
        }
        #expect(bridge.sent.isEmpty, "nothing reached the host")
        // Without a worker, a level 11 host is asked as before.
        try await session.ask(session.workList[0], scope: .research)
        #expect(bridge.sent.contains { if case .requestAssistant(_, _, _, _, _, _, nil) = $0 { true } else { false } })
    }

    @Test func askAndCaptureNameTheStaffMember() async throws {
        let bridge = StaffBridge()
        let session = try await connected(bridge)
        try await session.ask(session.workList[0], scope: .compare, worker: "researcher")
        #expect(bridge.sent.contains {
            if case .requestAssistant(_, "w-00000001", _, .compare, nil, nil, "researcher") = $0 { true } else { false }
        })
        var draft = CaptureDraft(clientId: "k2", text: "Compare the offers", workspace: "work")
        draft.worker = "writer"
        try await session.capture(clientId: draft.clientId, text: draft.trimmed, workspace: "work", assist: draft.assist)
        #expect(bridge.sent.contains {
            if case .capture(_, "k2", _, _, "work", let assist?) = $0 { assist.worker == "writer" } else { false }
        })
    }

    /// The device remembers applied notes per step: step 2's does not mark step 1's.
    @Test func applyNotesSendsTheStepAndKeysItByStep() async throws {
        let bridge = StaffBridge()
        let session = try await connected(bridge)
        let request = try #require(session.requests(of: session.workList[0]).first)
        let second = try #require(request.result(ofStep: 2))
        let first = try #require(request.result(ofStep: 1))
        try await session.applyNotes(of: request, result: second)
        #expect(session.notesApplied(request, second))
        #expect(!session.notesApplied(request, first))
        #expect(bridge.planRequests.contains {
            if case .applyNotes(_, "q-00000001", 1, "base-1", 2) = $0 { true } else { false }
        })
    }

    @Test func approvingAHandoffStartsTheNextStepOnce() async throws {
        let bridge = StaffBridge()
        let session = try await connected(bridge)
        let item = try #require(session.inboxItems.first)
        let saved = try await session.answer(item, .approve)
        #expect(saved.answer == .approve)
        var request = try #require(session.requests(of: session.workList[0]).first)
        #expect(request.steps.map(\.worker) == ["chief", "researcher", "writer"])
        #expect(request.steps.last?.delivery == .pending)
        #expect(request.waitingHandoff == nil)
        #expect(session.handoffItem(for: request, step: 2) == nil, "answered")
        // A second approve, as from another device, returns the same request.
        _ = try await session.answer(item, .approve)
        request = try #require(session.requests(of: session.workList[0]).first)
        #expect(request.steps.count == 3)
        #expect(session.savedWork.isEmpty && session.savedRequests.isEmpty)
    }

    @Test func aRefusedHandoffKeepsTheHostsReasonAndTheItemOpen() async throws {
        let bridge = StaffBridge()
        bridge.handoffRefusal = AssistantBridge.budget
        let session = try await connected(bridge)
        let item = try #require(session.inboxItems.first)
        await #expect(throws: PaneSession.Failure.bridge(AssistantBridge.budget)) {
            try await session.answer(item, .approve)
        }
        #expect(session.inboxItems.first?.state == .open)
    }

    @Test func rejectingAHandoffEndsTheChainWithItsResults() async throws {
        let bridge = StaffBridge()
        let session = try await connected(bridge)
        let item = try #require(session.inboxItems.first)
        _ = try await session.answer(item, .reject)
        let request = try #require(session.requests(of: session.workList[0]).first)
        #expect(request.steps.count == 2)
        #expect(session.handoffItem(for: request, step: 2) == nil)
        #expect(session.handoffItems(for: request, step: 2).first?.answer == .reject)
    }
}
