import MuxaProtocol
import Testing

@testable import MuxaSession

struct RecoveryActionTests {
    @Test func actionRequiresKnownPhaseAndHostOffer() {
        let all = RestartRecoveryAction.allCases.map(\.rawValue) + ["future-action"]
        #expect(RestartOperation(id: "op", phase: "prepared", actions: all).recoveryActions == [.cancelPrepared])
        #expect(RestartOperation(id: "op", phase: "dispatching", actions: all).recoveryActions == [.leaveUnconfirmed])
        #expect(RestartOperation(id: "op", phase: "replaced", actions: all).recoveryActions == [.finishReplacement, .leaveUnconfirmed])
        #expect(RestartOperation(id: "op", phase: "delivering", actions: all).recoveryActions == [.leaveUnconfirmed])
        for phase in ["future", "finalized", "resolved"] {
            #expect(RestartOperation(id: "op", phase: phase, actions: all).recoveryActions.isEmpty)
        }
        #expect(RestartOperation(id: "op", phase: "prepared").recoveryActions.isEmpty)
        #expect(RestartOperation(id: "", phase: "prepared", actions: all).recoveryActions.isEmpty)
    }

    @Test func interruptedRunDoesNotClaimToBeRunning() {
        var record = InboxTests.record("a")
        record.runs = [AgentRun(id: 1, startedAt: 1, reason: .restart, worker: "Reviewer", outcome: "unconfirmed")]
        let item = InboxItem(record: record, sessions: [InboxTests.session("a", .running)])
        #expect(item.runLines(now: 3).first?.detail.hasSuffix("unconfirmed outcome") == true)
        record.runs[0].outcome = "future"
        #expect(InboxItem(record: record, sessions: []).runLines(now: 3).first?.detail.hasSuffix("unknown outcome") == true)
    }
}

@MainActor
struct RecoveryRequestTests {
    func connected(capable: Bool = true, phase: String = "prepared") async throws -> (PaneSession, FakeBridge, TaskRecord) {
        let capabilities = ["task-records", "task-handoff"] + (capable ? ["task-restart-recovery"] : [])
        let (session, bridge) = try await RestartAgentTests().connected(capabilities: capabilities)
        bridge.taskRecords[0].restartOperation = RestartOperation(id: "op-1", phase: phase,
            worker: "Reviewer", actions: RestartRecoveryAction.allCases.map(\.rawValue))
        await session.loadTasks()
        return (session, bridge, session.taskRecords[0])
    }

    @Test func explicitRecoverySendsDisplayedIdentityAndNoRestart() async throws {
        let (session, bridge, displayed) = try await connected()
        let saved = try await session.recoverRestart(displayed, operation: "op-1", action: .cancelPrepared)
        #expect(saved.restartOperation == nil)
        #expect(bridge.received.contains { if case .recoverRestart(_, displayed.id, displayed.rev, "op-1", .cancelPrepared) = $0 { true } else { false } })
        #expect(!bridge.received.contains { if case .restartAgent = $0 { true } else { false } })
    }

    @Test func staleRevisionIsSentUnchangedAndConflictRefreshesWithoutRetry() async throws {
        let (session, bridge, displayed) = try await connected()
        bridge.taskRecords[0].rev += 1
        do {
            _ = try await session.recoverRestart(displayed, operation: "op-1", action: .cancelPrepared)
            Issue.record("stale recovery must conflict")
        } catch PaneSession.Failure.bridge(let body) { #expect(body.code == .conflict) }
        #expect(session.taskRecords[0].rev == displayed.rev + 1)
        #expect(bridge.received.filter { if case .recoverRestart = $0 { true } else { false } }.count == 1)
    }

    @Test func unknownOrChangedOperationAndOldHostNeverReceiveRecovery() async throws {
        for (capable, phase) in [(false, "prepared"), (true, "future")] {
            let (session, bridge, record) = try await connected(capable: capable, phase: phase)
            do {
                _ = try await session.recoverRestart(record, operation: "op-1", action: .cancelPrepared)
                Issue.record("unavailable action must be refused")
            } catch PaneSession.Failure.bridge { }
            #expect(!bridge.received.contains { if case .recoverRestart = $0 { true } else { false } })
        }
        let (session, bridge, record) = try await connected()
        bridge.taskRecords[0].restartOperation?.phase = "future"
        await session.loadTasks()
        do {
            _ = try await session.recoverRestart(record, operation: "op-1", action: .cancelPrepared)
            Issue.record("changed phase must invalidate open confirmation")
        } catch PaneSession.Failure.bridge(let body) { #expect(body.code == .conflict) }
        do {
            _ = try await session.restartAgent(task: record.id, force: true)
            Issue.record("an unresolved operation must block normal restart")
        } catch PaneSession.Failure.bridge(let body) { #expect(body.code == .conflict) }
        #expect(!bridge.received.contains { if case .recoverRestart = $0 { true } else { false } })
        #expect(!bridge.received.contains { if case .restartAgent = $0 { true } else { false } })
    }
}
