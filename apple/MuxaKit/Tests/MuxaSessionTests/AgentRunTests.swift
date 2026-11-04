import MuxaProtocol
import Testing

@testable import MuxaSession

struct AgentRunTests {
    static let runs = [
        AgentRun(id: 1, agent: "claude-code", startedAt: 1000, endedAt: 1000 + 42 * 60, agentSession: "s1", reason: .start),
        AgentRun(id: 2, startedAt: 1000 + 42 * 60, reason: .restart),
    ]

    func item(runs: [AgentRun], state: AgentState?) -> MuxaSession.InboxItem {
        var record = InboxTests.record("a")
        record.runs = runs
        return MuxaSession.InboxItem(record: record, sessions: [InboxTests.session("a", state)])
    }

    @Test func runsAreNewestFirstWithTheirReasonAgeAndDuration() {
        let lines = item(runs: Self.runs, state: .idle).runLines(now: 1000 + 3900)
        #expect(lines.map(\.title) == ["Run 2 · unknown agent", "Run 1 · claude-code"])
        #expect(lines[0].detail == "Restarted · 23 min ago · still running")
        #expect(lines[1].detail == "Started · 1 h 5 min ago · 42 min")
        #expect(lines[1].agentSession == "s1")
    }

    @Test func anOpenRunIsNotSaidToRunWithoutALiveAgent() {
        // No adapter state, and an older run with no end.
        let none = item(runs: Self.runs, state: nil).runLines(now: 5000)
        #expect(none[0].detail.hasSuffix("end not recorded"))
        let older = [AgentRun(id: 1, startedAt: 1, reason: .start), AgentRun(id: 2, startedAt: 2, reason: .other("later"))]
        let lines = item(runs: older, state: .running).runLines(now: 30)
        #expect(lines[0].detail == "later · <1 min ago · still running")
        #expect(lines[1].detail.hasSuffix("end not recorded"))
    }

    @Test func durationsReadInMinutesAndHours() {
        #expect(RunLine.duration(59) == "<1 min")
        #expect(RunLine.duration(60) == "1 min")
        #expect(RunLine.duration(3600) == "1 h")
        #expect(RunLine.duration(3600 * 30 + 300) == "30 h 5 min")
    }

    @Test func anUnobservedReplacementNeedsForceWithoutBeingShownAsRunning() {
        let initial = [Self.runs[0]]
        #expect(!item(runs: initial, state: nil).restartNeedsForce)
        #expect(item(runs: initial, state: .running).restartNeedsForce)
        #expect(!item(runs: initial, state: .waiting).restartNeedsForce)
        let unknown = item(runs: Array(Self.runs.reversed()), state: nil)
        #expect(unknown.restartNeedsForce)
        #expect(unknown.agent == nil)
        #expect(unknown.group == .noAgent)
        #expect(!item(runs: Self.runs, state: .idle).restartNeedsForce)
        #expect(!item(runs: Self.runs, state: .waiting).restartNeedsForce)
        let newerStart = Self.runs + [AgentRun(id: 3, startedAt: 5000, reason: .start)]
        #expect(!item(runs: newerStart, state: nil).restartNeedsForce)
    }

    @Test func onlyARestartLeavesAHandoff() {
        #expect(item(runs: Self.runs, state: nil).hasHandoff)
        #expect(!item(runs: [Self.runs[0]], state: nil).hasHandoff)
    }
}

@MainActor
struct RestartAgentTests {
    func connected(capabilities: [String] = ["task-records", "task-handoff"]) async throws -> (PaneSession, FakeBridge) {
        let bridge = FakeBridge()
        bridge.capabilities = capabilities
        var record = TaskRecordTests.record
        record.runs = [AgentRun(id: 1, agent: "claude-code", startedAt: 1, reason: .start)]
        bridge.taskRecords = [record]
        let session = PaneSession()
        try await session.connect {
            bridge.start()
            return bridge
        }
        return (session, bridge)
    }

    @Test func aRestartReturnsTheRecordWithItsNewRunAndListsIt() async throws {
        let (session, bridge) = try await connected()
        #expect(session.canRestartAgents)
        let saved = try await session.restartAgent(task: "t-000001", force: false)
        #expect(saved.runs.map(\.id) == [1, 2])
        #expect(saved.runs[0].endedAt == 9)
        #expect(saved.runs[1].reason == .restart)
        #expect(session.taskRecords.first == saved)
        #expect(bridge.received.contains {
            if case .restartAgent(_, "t-000001", false, nil, nil) = $0 { true } else { false }
        })
    }

    @Test func aRunningAgentIsRefusedWithoutForce() async throws {
        let (session, bridge) = try await connected()
        bridge.runningAgents = ["t-000001"]
        await #expect(throws: PaneSession.Failure.bridge(ErrorBody(code: .badRequest, message: "the agent is running"))) {
            try await session.restartAgent(task: "t-000001", force: false)
        }
        #expect(session.taskRecords.first?.runs.count == 1)
        let saved = try await session.restartAgent(task: "t-000001", force: true)
        #expect(saved.runs.count == 2)
    }

    @Test func anUnobservedReplacementCanBeExplicitlyStoppedAndRestarted() async throws {
        let (session, bridge) = try await connected()
        let first = try await session.restartAgent(task: "t-000001", force: false)
        let item = MuxaSession.InboxItem(record: first, sessions: [])
        #expect(item.restartNeedsForce)
        #expect(item.agent == nil)
        let saved = try await session.restartAgent(task: first.id, force: item.restartNeedsForce)
        #expect(saved.runs.count == 3)
        #expect(bridge.received.contains {
            if case .restartAgent(_, "t-000001", true, nil, nil) = $0 { true } else { false }
        })
    }

    @Test func anOlderBridgeIsNeverAskedToRestart() async throws {
        let (session, bridge) = try await connected(capabilities: ["task-records"])
        #expect(!session.canRestartAgents)
        await #expect(throws: PaneSession.Failure.bridge(
            ErrorBody(code: .unsupported, message: PaneSession.restartUnsupported))) {
            try await session.restartAgent(task: "t-000001", force: false)
        }
        #expect(!bridge.received.contains { if case .restartAgent = $0 { true } else { false } })
    }
}
