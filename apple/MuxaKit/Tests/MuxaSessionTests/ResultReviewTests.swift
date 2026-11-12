import Foundation
import MuxaProtocol
import Testing

@testable import MuxaSession

/// A bridge with `result` items (`work` level 16, ADR 0058), as the host answers: approve
/// starts the hand-off, reply makes a new version asked of the same member, and both send
/// `work-saved` before `inbox-saved`.
final class ResultBridge: PlanBridge, @unchecked Sendable {
    /// Send is refused with this, and the item stays open.
    var sendRefusal: ErrorBody?

    /// `StaffBridge`'s staff and chain, with a `result` item in place of the `handoff` one.
    override init() {
        super.init()
        assistant = AssistantInfo(overall: "assistant", delegates: [
            DelegateInfo(workspace: "work", share: .summaries, overallRequests: .review, staff: StaffTests.staff),
        ])
        work[0].requests = ["q-00000001"]
        requests = [StaffTests.chained]
        items = [ResultReviewTests.resultItem]
    }

    override func send(_ line: Data) async throws {
        let req = try Wire.decodeRequest(line)
        guard case .updateInboxEntry(let id, let entry, let action, _, let text, _, _, _) = req,
              let i = items.firstIndex(where: { $0.id == entry }), let review = items[i].result,
              let r = requests.firstIndex(where: { $0.id == review.request })
        else { return try await super.send(line) }
        switch action {
        case .approve:
            guard let h = review.handoff else { return }
            requests[r].steps.append(RequestStep(
                step: review.step + 1, worker: h.worker, by: .handoff, delivery: .pending, deliveryAt: 9, run: "t-run003"))
            emit(.workSaved(work: id, entry: work[0], request: requests[r]))
        case .reply:
            if let sendRefusal { return emit(.result(id: id, error: sendRefusal, sessions: nil)) }
            guard let text, !text.isEmpty else {
                return emit(.result(id: id, error: ErrorBody(code: .badRequest, message: "reply needs a text"), sessions: nil))
            }
            requests[r].version += 1
            requests[r].purpose = text
            requests[r].delivery = .pending
            requests[r].steps = [RequestStep(
                step: 1, worker: review.worker ?? "chief", by: .user, delivery: .pending, deliveryAt: 9, run: "t-run004")]
            emit(.workSaved(work: id, entry: work[0], request: requests[r]))
        default:
            break
        }
        items[i].state = .answered
        items[i].answer = action
        items[i].answers = []
        emit(.inboxSaved(inbox: id, entry: nil, item: items[i]))
        emit(.result(id: id, error: nil, sessions: nil))
    }
}

@MainActor
struct ResultReviewTests {
    nonisolated static let resultItem = InboxItem(
        id: "i-0000000r", kind: .result, state: .open, question: "researcher asks writer to: Draft the memo.",
        workspace: "work", title: "Plan the migration", createdAt: 8, summary: "Researched.",
        result: ResultReview(
            request: "q-00000001", version: 1, step: 2, work: "w-00000001", worker: "researcher",
            findings: ["Provider B covers every format."], next: ["Draft the memo."], files: ["/work/notes.md"],
            handoff: Handoff(worker: "writer", purpose: "Draft the memo.")),
        answers: [.approve, .reply, .dismiss, .later])

    func connected(_ bridge: ResultBridge, level: String = "work:16") async throws -> PaneSession {
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

    @Test func aResultItemOffersTheHandoffOfItsStep() async throws {
        let session = try await connected(ResultBridge())
        #expect(session.reviewsResults)
        let request = try #require(session.requests(of: session.workList[0]).first)
        #expect(session.handoffItem(for: request, step: 2)?.id == "i-0000000r")
        #expect(session.handoffItem(for: request, step: 1) == nil)
    }

    @Test func aLevel15HostMakesNoResultItems() async throws {
        let session = try await connected(ResultBridge(), level: "work:15")
        #expect(!session.reviewsResults)
    }

    @Test func handOffStartsTheNextStep() async throws {
        let session = try await connected(ResultBridge())
        let item = try #require(session.inboxItems.first)
        let saved = try await session.answer(item, .approve)
        #expect(saved.answer == .approve)
        let request = try #require(session.requests(of: session.workList[0]).first)
        #expect(request.steps.map(\.worker) == ["chief", "researcher", "writer"])
        #expect(session.savedWork.isEmpty && session.savedRequests.isEmpty)
    }

    @Test func sendAsksTheSameMemberAsANewVersion() async throws {
        let bridge = ResultBridge()
        let session = try await connected(bridge)
        let item = try #require(session.inboxItems.first)
        let saved = try await session.answer(item, .reply, text: "Also check the rate limits.")
        #expect(saved.state == .answered && saved.answer == .reply)
        let request = try #require(session.requests(of: session.workList[0]).first)
        #expect(request.version == 2 && request.purpose == "Also check the rate limits.")
        #expect(request.steps.first?.worker == "researcher" && request.steps.first?.by == .user)
        #expect(session.handoffItem(for: request, step: 2) == nil, "the newer version offers no hand-off")
    }

    /// A budget reached keeps the host's reason, and the item stays open.
    @Test func aRefusedSendKeepsTheHostsReason() async throws {
        let bridge = ResultBridge()
        bridge.sendRefusal = AssistantBridge.budget
        let session = try await connected(bridge)
        let item = try #require(session.inboxItems.first)
        await #expect(throws: PaneSession.Failure.bridge(AssistantBridge.budget)) {
            try await session.answer(item, .reply, text: "Again.")
        }
        #expect(session.inboxItems.first?.state == .open)
    }

    @Test func doneClosesTheItemAndStartsNothing() async throws {
        let session = try await connected(ResultBridge())
        let item = try #require(session.inboxItems.first)
        let saved = try await session.answer(item, .dismiss)
        #expect(saved.answer == .dismiss)
        let request = try #require(session.requests(of: session.workList[0]).first)
        #expect(request.steps.count == 2)
        #expect(session.handoffItems(for: request, step: 2).first?.answer == .dismiss)
    }

    /// Hand off, the reply box Send, ✓ Done and Later; Revise on a write (ADR 0058 §2, §4).
    @Test func theAnswersAreLaidOutAndNamed() {
        let item = Self.resultItem
        let answers = ItemAnswers(item)
        #expect(answers.decisions == [.approve] && answers.text == .reply && answers.done == .dismiss && answers.later)
        #expect(InboxAction.approve.title(for: item) == "Hand off to writer")
        #expect(InboxAction.reply.title(for: item) == "Send")
        #expect(InboxAction.dismiss.title(for: item) == "Done")
        #expect(item.askedAgain == "researcher")
        var plain = item
        plain.result?.handoff = nil
        plain.result?.worker = nil
        plain.answers = [.reply, .dismiss, .later]
        #expect(ItemAnswers(plain).decisions.isEmpty && plain.askedAgain == nil)

        let write = InboxItem(
            id: "i-0000000w", kind: .write, state: .open, question: "researcher proposes to write to slack:#x",
            workspace: "work", title: "Post", createdAt: 8,
            write: WriteProposal(request: "q-00000001", version: 1, step: 2, worker: "researcher", action: "Post",
                                 target: "slack:#x", content: "Hi", hash: "sha256:0"),
            answers: [.approve, .reject, .reply, .later])
        let revise = ItemAnswers(write)
        #expect(revise.text == .reply && revise.decisions == [.reject], "Approve only where the write shows whole")
        #expect(InboxAction.reply.title(for: write) == "Revise")
        #expect(write.askedAgain == "researcher")
    }
}
