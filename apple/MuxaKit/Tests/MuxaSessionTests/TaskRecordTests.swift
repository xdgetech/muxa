import Foundation
import MuxaProtocol
import Testing

@testable import MuxaSession

@MainActor
struct TaskRecordTests {
    func connected(_ bridge: FakeBridge) async throws -> PaneSession {
        let session = PaneSession()
        try await session.connect {
            bridge.start()
            return bridge
        }
        return session
    }

    static let record = TaskRecord(
        id: "t-000001", rev: 1, workspace: "work", title: "Fix it", goal: "Make it work", status: .open,
        createdAt: 1, updatedAt: 1)

    @Test func theListIsLoadedOnConnectWhenTheBridgeKeepsTasks() async throws {
        let bridge = FakeBridge()
        bridge.capabilities = ["task-records"]
        bridge.taskRecords = [Self.record]
        let session = try await connected(bridge)
        #expect(session.canKeepTasks)
        #expect(session.taskRecords == [Self.record])
        #expect(session.workspaces.map(\.name) == ["work"])
    }

    @Test func anOlderBridgeIsNeverAskedForTasks() async throws {
        let bridge = FakeBridge()
        let session = try await connected(bridge)
        #expect(!session.canKeepTasks)
        #expect(session.taskRecords.isEmpty)
        #expect(!bridge.received.contains { if case .listTasks = $0 { true } else { false } })
    }

    @Test func sessionsChangedListsTheTasksAgain() async throws {
        let bridge = FakeBridge()
        bridge.capabilities = ["task-records"]
        let session = try await connected(bridge)
        #expect(session.taskRecords.isEmpty)
        bridge.taskRecords = [Self.record]
        bridge.emit(.sessionsChanged)
        try await waitUntil { session.taskRecords == [Self.record] }
    }
}

@MainActor
struct TaskWriteTests {
    func connected(capabilities: [String] = ["task-records"]) async throws -> (PaneSession, FakeBridge) {
        let bridge = FakeBridge()
        bridge.capabilities = capabilities
        bridge.taskRecords = [TaskRecordTests.record]
        let session = PaneSession()
        try await session.connect {
            bridge.start()
            return bridge
        }
        return (session, bridge)
    }

    @Test func aCreatedTaskIsReturnedAndListed() async throws {
        let (session, bridge) = try await connected()
        let record = try await session.createTask(NewTaskRecord(workspace: "work", title: "Write", goal: "Draft it"))
        #expect(record.title == "Write")
        #expect(record.note == nil)
        #expect(session.taskRecords.contains(record))
        #expect(bridge.received.contains {
            if case .createTask(_, "work", "Write", "Draft it", nil, nil, nil, nil, nil, nil, nil, nil) = $0 { true } else { false }
        })
    }

    @Test func aFailedLaunchStillReturnsTheRecordWithItsNote() async throws {
        let (session, _) = try await connected()
        let record = try await session.createTask(
            NewTaskRecord(workspace: "work", title: "T", goal: "G", repo: "/r", branch: "taken"))
        #expect(record.note == "the branch exists")
    }

    @Test func aRefusedTaskIsAnError() async throws {
        let (session, _) = try await connected()
        await #expect(throws: PaneSession.Failure.bridge(ErrorBody(code: .badRequest, message: "no such workspace"))) {
            try await session.createTask(NewTaskRecord(workspace: "nope", title: "T", goal: "G"))
        }
    }

    @Test func aStatusChangeReturnsTheNewRev() async throws {
        let (session, _) = try await connected()
        let saved = try await session.updateTask(TaskRecordTests.record, status: .done)
        #expect(saved.rev == 2)
        #expect(saved.status == .done)
        #expect(session.taskRecords.first?.status == .done)
    }

    @Test func aConflictReloadsTheListAndSaysSo() async throws {
        let (session, bridge) = try await connected()
        // Another device marked it for review.
        bridge.taskRecords[0].rev = 2
        bridge.taskRecords[0].status = .review
        await #expect(throws: PaneSession.Failure.bridge(ErrorBody(code: .conflict, message: PaneSession.taskConflict))) {
            try await session.updateTask(TaskRecordTests.record, status: .done)
        }
        #expect(session.taskRecords.first?.rev == 2)
        #expect(session.taskRecords.first?.status == .review)
    }

    /// Done at `work` level 7 removes the worktree; work that could be lost needs a yes (ADR 0043).
    @Test func doneAsksBeforeItLosesWork() async throws {
        let (session, bridge) = try await connected(capabilities: ["task-records", "work:7"])
        bridge.unmerged = [TaskRecordTests.record.id]
        do {
            _ = try await session.updateTask(TaskRecordTests.record, status: .done)
            Issue.record("unmerged work must be refused")
        } catch {
            #expect(PaneSession.lostWork(error) == "its branch is not merged")
        }
        #expect(session.taskRecords.first?.status == .open, "nothing changed")
        let saved = try await session.updateTask(TaskRecordTests.record, status: .done, worktree: .force)
        #expect(saved.status == .done)
        let ends = bridge.received.compactMap { if case .updateTask(_, _, _, _, let end) = $0 { end } else { nil } }
        #expect(ends == [.remove, .force])
    }

    @Test func onlyAnEndSendsTheWorktreeAndOnlyToALevel7Host() async throws {
        let (session, bridge) = try await connected(capabilities: ["task-records", "work:7"])
        let review = try await session.updateTask(TaskRecordTests.record, status: .review)
        _ = try await session.updateTask(review, status: .dropped)
        let (older, olderBridge) = try await connected(capabilities: ["task-records", "work:6"])
        _ = try await older.updateTask(TaskRecordTests.record, status: .done)
        let sent = { (b: FakeBridge) in b.received.compactMap { if case .updateTask(_, _, _, let s, let end) = $0 { (s, end) } else { nil } } }
        #expect(sent(bridge).map(\.0) == [.review, .dropped])
        #expect(sent(bridge).map(\.1) == [nil, .remove])
        #expect(sent(olderBridge).map(\.1) == [nil])
        #expect(PaneSession.removeAnyway("its branch is not merged")
            == "Its branch is not merged. Remove its worktree and branch anyway?")
    }
}
