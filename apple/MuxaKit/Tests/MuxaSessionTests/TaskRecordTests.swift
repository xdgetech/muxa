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
