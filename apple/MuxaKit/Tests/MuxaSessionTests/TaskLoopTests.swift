import MuxaProtocol
import Testing

@testable import MuxaSession

/// The Task Loop through `PaneSession`, and the words the app shows for it (ADR 0019).
@MainActor
struct TaskLoopTests {
    nonisolated static func loop(
        _ state: LoopState, _ reason: LoopReason? = nil, attempts: UInt32 = 1, lastCheck: LoopCheck? = nil
    ) -> TaskLoop {
        TaskLoop(check: "make check", maxAttempts: 3, timeout: 600, state: state, attempts: attempts,
                 armedAt: 10, since: 10, reason: reason, lastCheck: lastCheck)
    }

    nonisolated static func record(loop: TaskLoop? = nil) -> TaskRecord {
        TaskRecord(id: "t-1", rev: 1, workspace: "work", title: "Title", goal: "Goal", status: .open,
                   createdAt: 1, updatedAt: 1, loop: loop)
    }

    func connected(capabilities: [String] = ["task-records", "task-loop"], records: [TaskRecord] = [])
        async throws -> (PaneSession, FakeBridge)
    {
        let bridge = FakeBridge()
        bridge.capabilities = capabilities
        bridge.taskRecords = records
        let session = PaneSession()
        try await session.connect {
            bridge.start()
            return bridge
        }
        return (session, bridge)
    }

    @Test func aStartedLoopReplacesTheListedRecord() async throws {
        let (session, bridge) = try await connected(records: [Self.record(loop: Self.loop(.stopped, .limit, attempts: 3))])
        #expect(session.canLoop)
        let saved = try await session.startLoop(session.taskRecords[0], loop: LoopSpec(check: "make check", maxAttempts: 5))
        #expect(saved.loop?.state == .armed)
        #expect(saved.loop?.attempts == 3)
        #expect(saved.loop?.maxAttempts == 5)
        #expect(session.taskRecords == [saved])
        guard case .startLoop(_, "t-1", let spec, false) = bridge.received.last else {
            Issue.record("no start-loop sent")
            return
        }
        #expect(spec == LoopSpec(check: "make check", maxAttempts: 5))
    }

    @Test func aResetCountsAttemptsFromZero() async throws {
        let (session, _) = try await connected(records: [Self.record(loop: Self.loop(.stopped, .limit, attempts: 3))])
        let saved = try await session.startLoop(session.taskRecords[0], loop: LoopSpec(check: "make check"), reset: true)
        #expect(saved.loop?.attempts == 0)
    }

    @Test func aStoppedLoopIsStoppedByTheUser() async throws {
        let (session, _) = try await connected(records: [Self.record(loop: Self.loop(.armed))])
        let saved = try await session.stopLoop(session.taskRecords[0])
        #expect(saved.loop?.state == .stopped)
        #expect(saved.loop?.reason == .user)
        #expect(session.taskRecords.first?.rev == 2)
    }

    @Test func aBridgeWithoutTaskLoopIsNeverAsked() async throws {
        let (session, bridge) = try await connected(capabilities: ["task-records"], records: [Self.record()])
        #expect(!session.canLoop)
        let calls: [() async throws -> Void] = [
            { _ = try await session.startLoop(Self.record(), loop: LoopSpec(check: "make check")) },
            { _ = try await session.stopLoop(Self.record()) },
            { _ = try await session.createTask(NewTaskRecord(workspace: "work", title: "T", goal: "G",
                                                             loop: LoopSpec(check: "make check"))) },
        ]
        for call in calls {
            await #expect {
                try await call()
            } throws: { error in
                guard case PaneSession.Failure.bridge(let body) = error else { return false }
                return body.code == .unsupported && PaneSession.describe(error) == loopNeedsDaemon
            }
        }
        #expect(!bridge.received.contains {
            switch $0 {
            case .startLoop, .stopLoop, .createTask: true
            default: false
            }
        })
    }

    @Test func theHostsRefusalSaysWhyAndKeepsTheRecord() async throws {
        let (session, bridge) = try await connected(records: [Self.record()])
        await #expect(throws: PaneSession.Failure.self) {
            try await session.startLoop(Self.record(), loop: LoopSpec(check: "rm -rf /"))
        }
        bridge.daemon = false
        do {
            _ = try await session.startLoop(Self.record(), loop: LoopSpec(check: "make check"))
            Issue.record("the daemon went away")
        } catch {
            #expect(PaneSession.describe(error) == loopNeedsDaemon)
        }
        #expect(session.taskRecords.first?.rev == 1)
    }

    @Test func aNewTaskCarriesItsLoop() async throws {
        let (session, bridge) = try await connected()
        let draft = TaskRecordDraft(
            workspace: "work", title: "T", goal: "G", loop: LoopDraft(checks: .list(["make check"]), maxAttempts: 4))
        let saved = try await session.createTask(try #require(draft.request))
        #expect(saved.loop?.state == .armed)
        #expect(bridge.received.contains {
            if case .createTask(_, _, _, _, _, _, _, _, LoopSpec(check: "make check", maxAttempts: 4), nil) = $0 { true } else { false }
        })
    }

    @Test func tasksChangedListsTheTasksAgain() async throws {
        let (session, bridge) = try await connected()
        bridge.taskRecords = [Self.record(loop: Self.loop(.passed))]
        bridge.emit(.tasksChanged)
        try await waitUntil { session.taskRecords.first?.loop?.state == .passed }
    }
}

struct LoopDraftTests {
    @Test func aDraftFollowsTheWorkspacesChecks() {
        let listed = LoopDraft(checks: .list(["make check", "cargo test"]))
        #expect(listed.check == "make check")
        #expect(listed.problem == nil)
        #expect(LoopDraft(checks: listed.checks, check: "make").problem == "Pick a check.")
        #expect(LoopDraft(checks: listed.checks, check: " make check").problem == "Pick a check.")
        #expect(LoopDraft(checks: listed.checks, maxAttempts: 11).problem == "Max attempts must be 1 to 10.")

        #expect(LoopDraft(checks: .any).problem == "Enter a check command.")
        #expect(LoopDraft(checks: .any, check: "  npm test ").spec == LoopSpec(check: "npm test", maxAttempts: 3))
        #expect(LoopDraft(checks: .any, check: "a\u{7}b").problem == "The check command cannot contain control characters.")
        #expect(LoopDraft(checks: .any, check: String(repeating: "x", count: 4097)).problem == "The check command is too long.")

        #expect(LoopDraft(checks: nil).problem == LoopDraft.noChecks)
        #expect(TaskRecordDraft(workspace: "work", title: "T", goal: "G", loop: LoopDraft(checks: nil)).problem
            == LoopDraft.noChecks)
    }
}

struct LoopLabelTests {
    typealias T = TaskLoopTests

    @Test func theBadgeNamesTheStateAndAttempts() {
        #expect(T.loop(.armed).badge == "Loop 1/3")
        #expect(T.loop(.checking).badge == "Loop checking")
        #expect(T.loop(.paused, .waiting).badge == "Loop paused")
        #expect(T.loop(.passed).badge == "Loop passed")
        #expect(T.loop(.stopped, .limit).badge == "Loop stopped: limit")
        #expect(T.loop(.stopped, .noAgent).badge == "Loop stopped: no agent")
        #expect(T.loop(.stopped, .user).badge == "Loop stopped")
        #expect(T.loop(.other("sleeping")).badge == "Loop: sleeping")
    }

    @Test func aPausedLoopSaysItResumesAfterTheTurn() {
        #expect(T.loop(.paused, .waiting).explanation
            == "The agent waits for you. The loop resumes after the agent finishes its turn.")
        #expect(T.loop(.stopped, .limit).explanation == "Stopped: the check failed 3 times.")
        #expect(T.loop(.stopped, .timeout).explanation == "Stopped: a check ran longer than 10 min.")
        #expect(T.loop(.stopped, .other("nap")).explanation == "Stopped: nap.")
        #expect(T.loop(.passed, lastCheck: LoopCheck(attempt: 2, exit: 0, seconds: 30, at: 100)).explanation
            == "The check passed on attempt 2.")
        #expect(T.loop(.other("sleeping")).explanation == "The host reports the state \"sleeping\".")
        #expect(T.loop(.other("sleeping")).stateTitle == "sleeping")
    }

    @Test func theLastCheckSaysExitOrTimedOut() {
        #expect(LoopCheck(attempt: 2, exit: 1, seconds: 42, at: 1000).line(now: 1300) == "Attempt 2 · exit 1 · 42 s · 5 min ago")
        #expect(LoopCheck(attempt: 3, exit: nil, seconds: 600, at: 1000).line(now: 1010)
            == "Attempt 3 · timed out · 10 min · <1 min ago")
    }

    @Test func onlyAnArmedCheckingOrPausedLoopCanBeStopped() {
        #expect(T.loop(.armed).isActive)
        #expect(T.loop(.paused).isActive)
        #expect(!T.loop(.passed).isActive)
        #expect(!T.loop(.stopped).isActive)
        #expect(!T.loop(.other("sleeping")).isKnownState)
    }
}
