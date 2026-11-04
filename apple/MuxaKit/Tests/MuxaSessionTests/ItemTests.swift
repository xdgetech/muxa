import Foundation
import MuxaProtocol
import Testing

@testable import MuxaSession

/// A bridge with Inbox items (ADR 0033), as the host answers them.
final class ItemBridge: FakeBridge, @unchecked Sendable {
    var items: [InboxItem] = [
        ItemTests.item("i-00000001", .ask, answers: [.approve, .reject, .later]),
        ItemTests.item("i-00000002", .review, answers: [.accept, .redo, .changeCourse, .keepOpen, .later]),
        ItemTests.item("i-00000003", .review, state: .answered),
    ]
    /// The ask's menu changed: `approve` is a conflict.
    var promptChanged = false

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
        case .updateInboxEntry(let id, let entry, let action, _, let text, _):
            record(req)
            guard let i = items.firstIndex(where: { $0.id == entry }) else { return refuse(id, .badRequest, "no such item") }
            if action == .read {
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
        default:
            try await super.send(line)
        }
    }

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
        let saved = try await session.answer(session.openItems[1], .redo, text: "Keep the old export.")
        #expect(saved.state == .answered)
        #expect(saved.answer == .redo)
        #expect(session.openItems.map(\.id) == ["i-00000001"])
        #expect(bridge.sent.contains {
            if case .updateInboxEntry(_, "i-00000002", .redo, nil, "Keep the old export.", nil) = $0 { true } else { false }
        })
    }

    @Test func theHostsRefusalIsShownAndTheListIsReadAgain() async throws {
        let bridge = ItemBridge()
        let session = try await connected(bridge)
        let lists = bridge.sent.filter { if case .listInbox = $0 { true } else { false } }.count
        do {
            try await session.answer(session.openItems[1], .redo, text: "")
            Issue.record("an empty Redo must be refused")
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
        let reads = bridge.sent.filter { if case .updateInboxEntry(_, _, .read, _, _, _) = $0 { true } else { false } }
        #expect(reads.count == 1)
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
        let review = ItemAnswers(Self.item("i-1", .review, answers: [.accept, .redo, .changeCourse, .keepOpen, .later]))
        #expect(review.shown == [.accept, .redo])
        #expect(review.more == [.changeCourse, .keepOpen, .later])
        #expect(review.main == .accept)

        let failure = ItemAnswers(Self.item("i-2", .failure, answers: [.retry, .changeCourse, .drop, .dismiss, .later]))
        #expect(failure.shown == [.retry, .changeCourse])
        #expect(failure.more == [.drop, .dismiss, .later])

        let question = ItemAnswers(Self.item("i-3", .ask, answers: [.choose, .later]))
        #expect(question.chooses)
        #expect(question.shown.isEmpty)
        #expect(question.main == nil)

        let briefing = ItemAnswers(Self.item("e-1", .briefing, answers: [.dismiss, .later]))
        #expect(briefing.shown == [.dismiss])
        #expect(InboxAction.dismiss.title(for: .briefing) == "Done")
        #expect(InboxAction.redo.needsText && !InboxAction.accept.needsText)
    }

    @Test func theRowShowsTheSourceAndTheAge() {
        var item = Self.item("i-1", .review)
        #expect(item.source(now: 100 + 240) == "Title i-1 · 4m")
        item.routine = "Morning brief"
        #expect(item.source(now: 100 + 7200) == "Morning brief · 2h")
        #expect(item.isUnread)
    }
}
