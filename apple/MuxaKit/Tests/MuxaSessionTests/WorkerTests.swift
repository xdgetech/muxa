import MuxaProtocol
import Testing

@testable import MuxaSession

struct WorkerChoicesTests {
    @Test func scopesDistinguishMissingAndEmpty() {
        #expect(WorkerInfo(name: "All", agent: "codex").allows(workspace: "work"))
        #expect(!WorkerInfo(name: "None", agent: "codex", workspaces: []).allows(workspace: "work"))
        #expect(WorkerInfo(name: "One", agent: "codex", workspaces: ["work"]).allows(workspace: "work"))
    }

    @Test func labelsDoNotGuessALegacyRunsWorker() {
        #expect(workerLabel(defaultWorker) == "Workspace default")
        #expect(workerLabel("Reviewer") == "Reviewer")
        #expect(workerLabel(nil) == "Worker unrecorded")
    }
}

@MainActor
struct WorkerRequestTests {
    @Test func restartSendsDisplayedRevisionWithKeepDefaultOrAName() async throws {
        let (session, bridge) = try await RestartAgentTests().connected(capabilities: ["task-records", "task-handoff", "workers"])
        bridge.workers = [WorkerInfo(name: "Reviewer", agent: "codex", workspaces: ["work"])]
        await session.loadTasks()
        for selection: String? in [nil, defaultWorker, "Reviewer"] {
            _ = try await session.restartAgent(task: "t-000001", force: false, worker: selection, rev: 42)
            #expect(bridge.received.contains {
                if case .restartAgent(_, "t-000001", false, let worker, 42) = $0 { worker == selection } else { false }
            })
        }
    }

    @Test func listingAndCreateCarryPublicWorkerMetadataAndSelection() async throws {
        let (session, bridge) = try await RestartAgentTests().connected(capabilities: ["task-records", "task-handoff", "workers"])
        bridge.workers = [WorkerInfo(name: "Reviewer", agent: "codex", workspaces: ["work"])]
        await session.loadTasks()
        #expect(session.workers == bridge.workers)
        _ = try await session.createTask(NewTaskRecord(workspace: "work", title: "Title", goal: "Goal", worker: "Reviewer"))
        #expect(bridge.received.contains {
            if case .createTask(_, "work", "Title", "Goal", _, _, _, _, _, "Reviewer") = $0 { true } else { false }
        })
    }

    @Test func metadataRefreshInvalidatesAnOpenSheetsWorkerSelection() async throws {
        let (session, bridge) = try await RestartAgentTests().connected(capabilities: ["task-records", "task-handoff", "workers"])
        bridge.workers = [WorkerInfo(name: "Reviewer", agent: "codex", workspaces: ["work"])]
        await session.loadTasks()
        let draft = NewTaskRecord(workspace: "work", title: "Title", goal: "Goal", worker: "Reviewer")
        bridge.workers = [WorkerInfo(name: "Reviewer", agent: "codex", workspaces: ["other"])]
        await session.loadTasks()
        do {
            _ = try await session.createTask(draft)
            Issue.record("changed scope must invalidate retained selection")
        } catch PaneSession.Failure.bridge(let body) { #expect(body.code == .badRequest) }
        bridge.workers = []
        await session.loadTasks()
        do {
            _ = try await session.restartAgent(task: "t-000001", force: false, worker: "Reviewer", rev: 1)
            Issue.record("removed worker must not be sent")
        } catch PaneSession.Failure.bridge(let body) { #expect(body.code == .badRequest) }
        #expect(!bridge.received.contains { if case .createTask = $0 { true } else { false } })
        #expect(!bridge.received.contains { if case .restartAgent = $0 { true } else { false } })
        #expect(workerProblem(name: nil, workspace: "other", workers: []) == nil)
        #expect(workerProblem(name: defaultWorker, workspace: "other", workers: []) == nil)
        #expect(workerProblem(name: "Reviewer", workspace: "other", workers: [WorkerInfo(name: "Reviewer", agent: "codex", workspaces: ["work"])]) != nil)
    }

    @Test func workerIntentNeverReachesAnOlderHost() async throws {
        let (session, bridge) = try await RestartAgentTests().connected()
        do {
            _ = try await session.restartAgent(task: "t-000001", force: false, worker: defaultWorker, rev: 1)
            Issue.record("worker intent must be refused locally")
        } catch PaneSession.Failure.bridge(let body) {
            #expect(body.code == .unsupported)
        }
        do {
            _ = try await session.createTask(NewTaskRecord(workspace: "work", title: "Title", goal: "Goal", worker: "Reviewer"))
            Issue.record("worker create must be refused locally")
        } catch PaneSession.Failure.bridge(let body) {
            #expect(body.code == .unsupported)
        }
        #expect(!bridge.received.contains { if case .restartAgent = $0 { true } else { false } })
        #expect(!bridge.received.contains { if case .createTask = $0 { true } else { false } })
    }
}
