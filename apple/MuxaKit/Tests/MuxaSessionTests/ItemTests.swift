import Foundation
import MuxaProtocol
import Testing

@testable import MuxaSession

/// A bridge with Inbox items (ADR 0033), as the host answers them.
final class ItemBridge: FakeBridge, @unchecked Sendable {
    var items: [InboxItem] = [
        ItemTests.item("i-00000001", .ask, answers: [.approve, .reject, .later]),
        ItemTests.item("i-00000002", .review, answers: [.accept, .reply, .keepOpen, .later]),
        ItemTests.item("i-00000003", .review, state: .answered),
    ]
    /// The ask's menu changed: `approve` is a conflict.
    var promptChanged = false
    /// An older host refuses `read` on a Routine's entry.
    var refusesRead = false

    override func send(_ line: Data) async throws {
        let req = try Wire.decodeRequest(line)
        switch req {
        case .listInbox(let id):
            record(req)
            emit(.inboxList(inbox: id, entries: [], items: items))
            emit(.result(id: id, error: nil, sessions: nil))
        case .listRoutines(let id):
            emit(.routineList(routines: id, entries: []))
            emit(.result(id: id, error: nil, sessions: nil))
        case .updateInboxEntry(let id, let entry, let action, _, let text, _, _):
            record(req)
            guard let i = items.firstIndex(where: { $0.id == entry }) else { return refuse(id, .badRequest, "no such item") }
            if action == .askAgent, items[i].answers.contains(.askAgent) {
                guard !(text ?? "").isEmpty else { return refuse(id, .badRequest, "a text is needed") }
                emit(.sessionStarted(start: id, session: "$9", name: "ask-w"))
                emit(.inboxSaved(inbox: id, entry: nil, item: items[i]))
                emit(.result(id: id, error: nil, sessions: nil))
                return
            }
            if action == .later, items[i].answers.contains(.later) {
                var moved = items.remove(at: i)
                moved.laterAt = 60
                let closed = items.firstIndex { $0.state != .open } ?? items.endIndex
                items.insert(moved, at: closed)
                emit(.inboxSaved(inbox: id, entry: nil, item: moved))
                emit(.result(id: id, error: nil, sessions: nil))
                return
            }
            if action == .protect || action == .unprotect {
                items[i].protectedAt = action == .protect ? 70 : nil
            } else if action == .read {
                if refusesRead { return refuse(id, .badRequest, "Read does not answer a Routine's entry") }
                items[i].readAt = 50
            } else {
                guard items[i].answers.contains(action) else { return refuse(id, .badRequest, "\(action.rawValue) is not offered") }
                if action.needsText, text?.isEmpty ?? true { return refuse(id, .badRequest, "a text is needed") }
                if promptChanged, items[i].kind == .ask { return refuse(id, .conflict, "the prompt changed") }
                items[i].state = .answered
                items[i].answer = action
                items[i].answers = []
            }
            emit(.inboxSaved(inbox: id, entry: nil, item: items[i]))
            emit(.result(id: id, error: nil, sessions: nil))
        case .deleteInbox(let id, let days):
            record(req)
            guard (1...3650).contains(days) else { return refuse(id, .badRequest, "olderThanDays must be 1 to 3650") }
            let cutoff = now - UInt64(days) * 86_400
            let before = items.count
            items.removeAll { $0.createdAt < cutoff && $0.protectedAt == nil }
            emit(.inboxDeleted(inbox: id, deleted: UInt32(before - items.count)))
            emit(.result(id: id, error: nil, sessions: nil))
        default:
            try await super.send(line)
        }
    }

    /// The host's clock for `delete-inbox`, epoch seconds.
    var now: UInt64 = 10 * 86_400

    private let lock = NSLock()
    private var _sent: [Request] = []
    var sent: [Request] { lock.withLock { _sent } }
    private func record(_ req: Request) { lock.withLock { _sent.append(req) } }

    private func refuse(_ id: UInt64, _ code: ErrorCode, _ message: String) {
        emit(.result(id: id, error: ErrorBody(code: code, message: message), sessions: nil))
    }
}

@MainActor
struct ItemTests {
    nonisolated static func item(
        _ id: String, _ kind: ItemKind, state: ItemState = .open, answers: [InboxAction] = []
    ) -> InboxItem {
        InboxItem(id: id, kind: kind, state: state, question: "Question \(id)", workspace: "work", title: "Title \(id)",
                  createdAt: 100, task: "t-\(id)", session: "$1", pane: "%1", answers: answers)
    }

    func connected(_ bridge: ItemBridge, level: String = "work:3") async throws -> PaneSession {
        bridge.capabilities = ["task-records", level]
        let session = PaneSession()
        try await session.connect {
            bridge.start()
            return bridge
        }
        return session
    }

    @Test func connectKeepsTheHostsItemsInItsOrder() async throws {
        let bridge = ItemBridge()
        let session = try await connected(bridge)
        #expect(session.canUseItems)
        #expect(session.inboxItems.map(\.id) == ["i-00000001", "i-00000002", "i-00000003"])
        #expect(session.openItems.map(\.id) == ["i-00000001", "i-00000002"])
    }

    @Test func anOlderHostHasNoItems() async throws {
        let bridge = ItemBridge()
        let session = try await connected(bridge, level: "work:2")
        #expect(!session.canUseItems)
        #expect(session.inboxItems.isEmpty)
        await #expect(throws: PaneSession.Failure.bridge(ErrorBody(code: .unsupported, message: PaneSession.itemsUnsupported))) {
            try await session.answer(bridge.items[1], .accept)
        }
        #expect(!bridge.sent.contains { if case .updateInboxEntry = $0 { true } else { false } })
    }

    @Test func anAnswerSendsItsTextAndTheItemLeavesTheList() async throws {
        let bridge = ItemBridge()
        let session = try await connected(bridge)
        let saved = try await session.answer(session.openItems[1], .reply, text: "Keep the old export.")
        #expect(saved.state == .answered)
        #expect(saved.answer == .reply)
        #expect(session.openItems.map(\.id) == ["i-00000001"])
        #expect(bridge.sent.contains {
            if case .updateInboxEntry(_, "i-00000002", .reply, nil, "Keep the old export.", nil, nil) = $0 { true } else { false }
        })
    }

    /// Complete Task ends the Task with its worktree at `work` level 7 (ADR 0043); Done does not.
    @Test func completeTaskRemovesTheWorktreeOnALevel7Host() async throws {
        let bridge = ItemBridge()
        bridge.items[0].answers = [.keepOpen]
        let session = try await connected(bridge, level: "work:7")
        try await session.answer(session.openItems[0], .keepOpen)
        try await session.answer(session.openItems[0], .accept)
        let sent = bridge.sent.compactMap { if case .updateInboxEntry(_, _, let a, _, _, _, let end) = $0 { (a, end) } else { nil } }
            .filter { $0.0 != .read }
        #expect(sent.map(\.0) == [.keepOpen, .accept])
        #expect(sent.map(\.1) == [nil, .remove])
    }

    @Test func theHostsRefusalIsShownAndTheListIsReadAgain() async throws {
        let bridge = ItemBridge()
        let session = try await connected(bridge)
        let lists = bridge.sent.filter { if case .listInbox = $0 { true } else { false } }.count
        do {
            try await session.answer(session.openItems[1], .reply, text: "")
            Issue.record("an empty Send must be refused")
        } catch {
            #expect(PaneSession.describe(error) == "a text is needed")
        }
        #expect(bridge.sent.filter { if case .listInbox = $0 { true } else { false } }.count == lists + 1)
    }

    @Test func aChangedPromptIsAConflict() async throws {
        let bridge = ItemBridge()
        bridge.promptChanged = true
        let session = try await connected(bridge)
        await #expect(throws: PaneSession.Failure.bridge(ErrorBody(code: .conflict, message: PaneSession.itemConflict))) {
            try await session.answer(session.openItems[0], .approve)
        }
        #expect(session.openItems.map(\.id) == ["i-00000001", "i-00000002"])
    }

    @Test func openingMarksReadOnce() async throws {
        let bridge = ItemBridge()
        let session = try await connected(bridge)
        await session.markRead(session.openItems[0])
        #expect(session.openItems[0].readAt == 50)
        #expect(session.openItems[0].state == .open)
        await session.markRead(session.openItems[0])
        let reads = bridge.sent.filter { if case .updateInboxEntry(_, _, .read, _, _, _, _) = $0 { true } else { false } }
        #expect(reads.count == 1)
    }

    @Test func laterMovesTheItemDownInTheHostsOrder() async throws {
        let bridge = ItemBridge()
        let session = try await connected(bridge)
        let saved = try await session.answer(session.openItems[0], .later)
        #expect(saved.laterAt == 60 && saved.state == .open)
        #expect(session.openItems.map(\.id) == ["i-00000002", "i-00000001"], "listed again, moved down, not hidden")
    }

    @Test func askAgentNamesTheSessionAndLeavesTheItemOpen() async throws {
        let bridge = ItemBridge()
        bridge.items[1].answers.insert(.askAgent, at: bridge.items[1].answers.count - 1)
        let session = try await connected(bridge)
        let started = try await session.askAgent(session.openItems[1], text: "Put this in Notion.")
        #expect(started == StartedSession(id: "$9", name: "ask-w"))
        #expect(session.openItems.map(\.id) == ["i-00000001", "i-00000002"], "the item stays open")
        #expect(bridge.sent.contains {
            if case .updateInboxEntry(_, "i-00000002", .askAgent, nil, "Put this in Notion.", nil, nil) = $0 { true } else { false }
        })
        // A host without Ask Agent lists no such answer: nothing is sent.
        await #expect(throws: PaneSession.Failure.self) { try await session.askAgent(session.openItems[0], text: "x") }
    }

    @Test func aRefusedReadIsNoErrorAndKeepsTheDot() async throws {
        let bridge = ItemBridge()
        bridge.refusesRead = true
        let session = try await connected(bridge)
        await session.markRead(session.openItems[0])
        #expect(session.workError == nil)
        #expect(session.openItems[0].readAt == nil)
    }

    @Test func tasksChangedListsTheItemsAgain() async throws {
        let bridge = ItemBridge()
        let session = try await connected(bridge)
        bridge.items.insert(Self.item("i-00000004", .failure, answers: [.retry]), at: 0)
        bridge.emit(.tasksChanged)
        for _ in 0..<100 where session.inboxItems.first?.id != "i-00000004" {
            try await Task.sleep(for: .milliseconds(10))
        }
        #expect(session.inboxItems.first?.id == "i-00000004")
    }

    @Test func answersFollowTheKindsTable() {
        let reviewItem = Self.item("i-1", .review, answers: [.accept, .reply, .keepOpen, .later])
        let review = ItemAnswers(reviewItem)
        #expect(review.decisions.isEmpty, "a review asks nothing to decide")
        #expect(review.text == .reply && review.menu == [.accept])
        #expect(review.done == .keepOpen && review.later)
        #expect(review.main == .accept)
        #expect(ItemAnswers(reviewItem, replying: true).main == .reply, "a typed reply is the main answer")

        let failure = ItemAnswers(Self.item("i-2", .failure, answers: [.retry, .reply, .drop, .dismiss, .later]))
        #expect(failure.decisions == [.retry] && failure.main == .retry)
        #expect(failure.text == .reply && failure.menu == [.drop] && failure.done == .dismiss)

        let ask = ItemAnswers(Self.item("i-5", .ask, answers: [.approve, .reject, .reply, .later]))
        #expect(ask.decisions == [.approve, .reject] && ask.text == .reply)
        #expect(ask.menu.isEmpty && ask.done == nil && ask.later)

        let question = ItemAnswers(Self.item("i-3", .ask, answers: [.choose, .later]))
        #expect(question.chooses && question.decisions.isEmpty && question.text == nil && question.main == nil)
        let cancellable = ItemAnswers(Self.item("i-9", .ask, answers: [.choose, .reject, .later]))
        #expect(cancellable.main == nil, "Command+Return never picks a question's cancel")

        let proposal = ItemAnswers(Self.item("e-3", .proposal, answers: [.approve, .reject, .askAgent, .later]))
        #expect(proposal.decisions == [.approve, .reject] && proposal.text == .askAgent)

        // ADR 0041: a briefing's reply box asks the workspace's agent.
        let briefingItem = Self.item("e-1", .briefing, answers: [.dismiss, .askAgent, .later])
        let briefing = ItemAnswers(briefingItem)
        #expect(briefing.decisions.isEmpty && briefing.menu.isEmpty && briefing.text == .askAgent)
        #expect(briefing.done == .dismiss && briefing.main == .dismiss)
        #expect(ItemAnswers(briefingItem, replying: true).main == .askAgent)
        #expect(InboxAction.reply.needsText && InboxAction.askAgent.needsText && !InboxAction.accept.needsText)
    }

    /// ADR 0033 §4a.2, ADR 0040: the labels name the result.
    @Test func labelsNameTheResult() {
        #expect(InboxAction.accept.title(for: .review) == "Complete Task")
        #expect(InboxAction.drop.title(for: .failure) == "Drop Task")
        #expect(InboxAction.keepOpen.title(for: .review) == "Done")
        #expect(InboxAction.dismiss.title(for: .briefing) == "Done")
        #expect(InboxAction.reply.title(for: .review) == "Send")
        #expect(InboxAction.reply.title(for: .failure) == "Send")
        #expect(InboxAction.reply.title(for: .ask) == "Deny & Send")
        #expect(InboxAction.approve.title(for: .ask) == "Allow")
        #expect(InboxAction.reject.title(for: .ask) == "Deny")
        #expect(InboxAction.approve.title(for: .proposal) == "Approve")
        #expect(InboxAction.reject.title(for: .proposal) == "Reject")
    }

    @Test func aReplySendsItsText() async throws {
        let bridge = ItemBridge()
        bridge.items[0].answers = [.approve, .reject, .reply, .later]
        let session = try await connected(bridge)
        try await session.answer(session.openItems[0], .reply, text: "Run terraform plan first.")
        #expect(bridge.sent.contains {
            if case .updateInboxEntry(_, "i-00000001", .reply, nil, "Run terraform plan first.", nil, nil) = $0 { true } else { false }
        })
    }

    /// The summary's web links and the attachments it names are links (ADR 0033 §4a.5).
    @Test func theSummaryLinksItsAttachmentsAndTheWeb() {
        let attachments = Attachments(docs: ["docs/plan.md", "notes/plan.md", "report.md"], images: ["shots/chart.png"])
        let summary = "Wrote report.md and **chart.png**. See [the PR](https://github.com/example/app/pull/1), "
            + "https://example.com/runs/42 and /work/app/docs/plan.md. [Local](file:///etc/hosts), plan.md, "
            + "[design](notes/plan.md)."
        let text = Self.rendered(ItemSummary.markdown(summary, attachments: attachments))
        let links = text.runs.compactMap { run in
            run.link.map { (String(text[run.range].characters), SummaryLink($0)) }
        }
        #expect(links.map(\.0) == ["report.md", "chart.png", "the PR", "https://example.com/runs/42",
                                    "/work/app/docs/plan.md", "design"])
        #expect(links.map(\.1) == [
            .doc("report.md"), .image("shots/chart.png"), .web(URL(string: "https://github.com/example/app/pull/1")!),
            .web(URL(string: "https://example.com/runs/42")!), .doc("docs/plan.md"), .doc("notes/plan.md"),
        ])
        #expect(String(text.characters).contains("Local, plan.md"), "a link that is not followed is text")
        #expect(SummaryLink(SummaryLink.doc("a b/c.md").url) == .doc("a b/c.md"))
    }

    /// Blocks stay as written; code is not linked, and a code span or image that names
    /// an attachment is a link.
    @Test func theSummaryKeepsItsBlocks() {
        let attachments = Attachments(docs: ["docs/plan.md", "notes/My Plan (v2).md"], images: ["shots/my_chart.png"])
        let summary = """
            ## Done
            - Read `plan.md` and `npm test`.
            > ![chart](shots/my_chart.png), *my_chart.png*

            ```
            cat docs/plan.md https://example.com
            ```
                docs/plan.md
            1. See `notes/My Plan (v2).md`.
            """
        let plan = SummaryLink.doc("docs/plan.md").url.absoluteString
        let chart = SummaryLink.image("shots/my_chart.png").url.absoluteString
        let mine = SummaryLink.doc("notes/My Plan (v2).md").url.absoluteString
        #expect(ItemSummary.markdown(summary, attachments: attachments) == """
            ## Done
            - Read [`plan.md`](<\(plan)>) and `npm test`.
            > [chart](<\(chart)>), *[my\\_chart.png](<\(chart)>)*

            ```
            cat docs/plan.md https://example.com
            ```
                docs/plan.md
            1. See [`notes/My Plan (v2).md`](<\(mine)>).
            """)
        let links = Self.rendered(ItemSummary.markdown(summary, attachments: attachments)).runs.compactMap(\.link)
        #expect(links.compactMap(SummaryLink.init) == [
            .doc("docs/plan.md"), .image("shots/my_chart.png"), .image("shots/my_chart.png"), .doc("notes/My Plan (v2).md"),
        ])
    }

    private static func rendered(_ markdown: String) -> AttributedString {
        (try? AttributedString(markdown: markdown)) ?? AttributedString(markdown)
    }

    @Test func theRowShowsTheSourceAndTheAge() {
        var item = Self.item("i-1", .review)
        #expect(item.source(now: 100 + 240) == "Title i-1 · 4m")
        item.routine = "Morning brief"
        #expect(item.source(now: 100 + 7200) == "Morning brief · 2h")
        #expect(item.isUnread)
    }
}
