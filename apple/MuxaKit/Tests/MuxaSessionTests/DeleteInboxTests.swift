import Foundation
import MuxaProtocol
import Testing

@testable import MuxaSession

/// Protect and Delete Old (ADR 0046) against `ItemBridge`, whose items are made at 100.
@MainActor
struct DeleteInboxTests {
    private let day: UInt64 = 86_400

    func connected(_ bridge: ItemBridge, level: String = "work:8") async throws -> PaneSession {
        bridge.capabilities = ["task-records", level]
        let session = PaneSession()
        try await session.connect {
            bridge.start()
            return bridge
        }
        return session
    }

    @Test func protectMarksTheItemAndLeavesItOpen() async throws {
        let bridge = ItemBridge()
        let session = try await connected(bridge)
        #expect(session.canDeleteInbox)
        let saved = try await session.setProtected(session.openItems[1], true)
        #expect(saved.isProtected)
        #expect(saved.state == .open)
        #expect(saved.answers == [.accept, .reply, .keepOpen, .later], "a protected item keeps its answers")
        #expect(session.openItems.map(\.id) == ["i-00000001", "i-00000002"])
        #expect(session.openItems[1].isProtected)
        try await session.setProtected(session.openItems[1], false)
        #expect(!session.openItems[1].isProtected)
        #expect(bridge.sent.contains {
            if case .updateInboxEntry(_, "i-00000002", .unprotect, nil, nil, nil, nil, _) = $0 { true } else { false }
        })
    }

    @Test func deleteOldReturnsTheCountAndListsAgain() async throws {
        let bridge = ItemBridge()
        bridge.items.append(ItemTests.item("i-00000004", .review, answers: [.keepOpen]))
        bridge.items[3].createdAt = bridge.now - day
        let session = try await connected(bridge)
        try await session.setProtected(session.openItems[0], true)
        #expect(session.deletableCount(olderThanDays: 3, now: bridge.now) == 2, "the protected and the new item stay")
        let lists = bridge.sent.filter { if case .listInbox = $0 { true } else { false } }.count
        let deleted = try await session.deleteOld(olderThanDays: 3)
        #expect(deleted == 2)
        #expect(session.inboxItems.map(\.id) == ["i-00000001", "i-00000004"])
        #expect(bridge.sent.filter { if case .listInbox = $0 { true } else { false } }.count == lists + 1)
        #expect(bridge.sent.contains { if case .deleteInbox(_, 3) = $0 { true } else { false } })
    }

    @Test func theHostsRefusalIsShown() async throws {
        let bridge = ItemBridge()
        let session = try await connected(bridge)
        do {
            try await session.deleteOld(olderThanDays: 0)
            Issue.record("0 days must be refused")
        } catch {
            #expect(PaneSession.describe(error) == "olderThanDays must be 1 to 3650")
        }
        #expect(session.inboxItems.count == 3)
    }

    @Test func anOlderHostCannotProtectOrDelete() async throws {
        let bridge = ItemBridge()
        let session = try await connected(bridge, level: "work:6")
        #expect(!session.canDeleteInbox)
        let unsupported = PaneSession.Failure.bridge(ErrorBody(code: .unsupported, message: PaneSession.deleteInboxUnsupported))
        await #expect(throws: unsupported) { try await session.setProtected(session.openItems[0], true) }
        await #expect(throws: unsupported) { try await session.deleteOld(olderThanDays: 1) }
        #expect(!bridge.sent.contains {
            switch $0 {
            case .deleteInbox, .updateInboxEntry: true
            default: false
            }
        })
    }

    @Test func onlyOldUnprotectedItemsAreDeletable() {
        var item = ItemTests.item("i-00000001", .review)
        item.createdAt = 10 * day
        #expect(!item.isDeletable(olderThanDays: 1, now: 11 * day), "exactly one day is not older")
        #expect(item.isDeletable(olderThanDays: 1, now: 11 * day + 1))
        #expect(!item.isDeletable(olderThanDays: 5, now: 2 * day), "a cutoff before the epoch")
        item.protectedAt = 1
        #expect(!item.isDeletable(olderThanDays: 1, now: 20 * day))
    }

    @Test func theConfirmationNamesTheCount() {
        #expect(PaneSession.deleteOldMessage(count: 12, days: 3) == "Delete 12 items older than 3 days? Protected items stay.")
        #expect(PaneSession.deleteOldMessage(count: 1, days: 1) == "Delete 1 item older than 1 day? Protected items stay.")
        #expect(PaneSession.deleteOldMessage(count: 0, days: 3) == "No unprotected items are older than 3 days.")
    }

    @Test func protectIsNeverAnAnswerOnTheScreen() {
        let item = ItemTests.item("i-00000001", .review, answers: [.protect, .unprotect, .keepOpen, .later])
        let answers = ItemAnswers(item)
        #expect(answers.main == .keepOpen)
        #expect(answers.menu.isEmpty && answers.decisions.isEmpty)
    }
}
