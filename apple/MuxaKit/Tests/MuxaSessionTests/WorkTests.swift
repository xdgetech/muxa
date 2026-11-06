import Foundation
import MuxaProtocol
import Testing

@testable import MuxaSession

/// A bridge with Routines and stored entries, as the host answers them.
final class WorkBridge: FakeBridge, @unchecked Sendable {
    var routines: [RoutineInfo] = [
        RoutineInfo(workspace: "work", name: "digest", trigger: RoutineTrigger(at: "08:00", timezone: "Europe/Berlin"),
                    enabled: true, nextDue: 2000),
        RoutineInfo(workspace: "work", name: "off", enabled: false),
    ]
    var entries: [InboxEntry] = [
        WorkTests.entry("e-00000001", kind: "proposal"),
        WorkTests.entry("e-00000002", kind: "briefing"),
    ]
    /// False: no daemon, so `run-routine` is `unsupported`.
    var routineDaemon = true

    override func send(_ line: Data) async throws {
        let req = try Wire.decodeRequest(line)
        switch req {
        case .listRoutines(let id):
            record(req)
            emit(.routineList(routines: id, entries: routines))
            emit(.result(id: id, error: nil, sessions: nil))
        case .listInbox(let id):
            record(req)
            emit(.inboxList(inbox: id, entries: entries))
            emit(.result(id: id, error: nil, sessions: nil))
        case .runRoutine(let id, let workspace, let name):
            record(req)
            guard routineDaemon else {
                return refuse(id, .unsupported, "Routines run in the muxa daemon. On the host, run: muxa-bridge daemon install")
            }
            guard let i = routines.firstIndex(where: { $0.workspace == workspace && $0.name == name }), routines[i].enabled else {
                return refuse(id, .badRequest, "the routine is disabled")
            }
            guard routines[i].running == nil else { return refuse(id, .conflict, "the routine is running") }
            routines[i].running = RoutineRun(occurrence: "now-1", startedAt: 1, task: "t-run1")
            emit(.routineSaved(routine: id, entry: routines[i]))
            emit(.result(id: id, error: nil, sessions: nil))
        case .updateInboxEntry(let id, let entry, let action, let until, _, _, _):
            record(req)
            guard let i = entries.firstIndex(where: { $0.id == entry }),
                  entries[i].disposition == "open" || entries[i].disposition == "later" || action == .approve
            else { return refuse(id, .badRequest, "the entry is closed") }
            switch action {
            case .approve:
                guard entries[i].kind == "proposal" else { return refuse(id, .badRequest, "a briefing has nothing to approve") }
                if entries[i].task == nil {
                    let task = TaskRecord(id: "t-ok\(taskRecords.count)", rev: 1, workspace: "work", title: entries[i].title,
                                          goal: "g", status: .open, createdAt: 1, updatedAt: 1)
                    taskRecords.append(task)
                    entries[i].task = task.id
                    entries[i].disposition = "approved"
                }
            case .dismiss: entries[i].disposition = "dismissed"
            case .later:
                entries[i].disposition = "later"
                entries[i].until = until
            default: return refuse(id, .badRequest, "not an entry's action")
            }
            emit(.inboxSaved(inbox: id, entry: entries[i]))
            emit(.result(id: id, error: nil, sessions: nil))
        default:
            try await super.send(line)
        }
    }

    private let lock = NSLock()
    private var _work: [Request] = []
    var work: [Request] { lock.withLock { _work } }
    private func record(_ req: Request) { lock.withLock { _work.append(req) } }

    private func refuse(_ id: UInt64, _ code: ErrorCode, _ message: String) {
        emit(.result(id: id, error: ErrorBody(code: code, message: message), sessions: nil))
    }
}

@MainActor
struct WorkTests {
    nonisolated static func entry(_ id: String, kind: String, disposition: String = "open", until: UInt64? = nil) -> InboxEntry {
        InboxEntry(id: id, workspace: "work", routine: "digest", key: id, kind: kind, title: "Title \(id)", body: "Body",
                   links: nil, createdAt: 1, run: "t-run0", occurrence: "2026-11-03T08:00", disposition: disposition,
                   until: until,
                   proposal: kind == "proposal" ? Proposal(action: "Close", target: "linear:ENG-1", content: "Done.", hash: "sha256:00") : nil,
                   task: nil)
    }

    func connected(_ bridge: WorkBridge) async throws -> PaneSession {
        let session = PaneSession()
        try await session.connect {
            bridge.start()
            return bridge
        }
        return session
    }

    @Test func connectListsRoutinesAndEntriesAtWorkLevelOne() async throws {
        let bridge = WorkBridge()
        bridge.capabilities = ["task-records", "work:1"]
        let session = try await connected(bridge)
        #expect(session.workLevel == 1)
        #expect(session.canUseRoutines)
        #expect(session.routines.map(\.name) == ["digest", "off"])
        #expect(session.inboxEntries.map(\.id) == ["e-00000001", "e-00000002"])
    }

    @Test func anOlderHostIsNeverAskedForRoutines() async throws {
        let bridge = WorkBridge()
        bridge.capabilities = ["task-records", "work:x"]
        let session = try await connected(bridge)
        #expect(session.workLevel == 0)
        #expect(session.routines.isEmpty)
        #expect(bridge.work.isEmpty)
        await #expect(throws: PaneSession.Failure.bridge(ErrorBody(code: .unsupported, message: PaneSession.routinesUnsupported))) {
            try await session.runRoutine(bridge.routines[0])
        }
        await #expect(throws: PaneSession.Failure.self) { try await session.dismiss(bridge.entries[1]) }
        #expect(bridge.work.isEmpty)
    }

    @Test func runNowKeepsTheRunAndShowsTheHostsRefusal() async throws {
        let bridge = WorkBridge()
        bridge.capabilities = ["work:2"]
        let session = try await connected(bridge)
        let saved = try await session.runRoutine(session.routines[0])
        #expect(saved.running?.task == "t-run1")
        #expect(session.routines[0].running?.task == "t-run1")
        // A second run while one is open is a conflict.
        await #expect(throws: PaneSession.Failure.self) { try await session.runRoutine(session.routines[0]) }

        bridge.routineDaemon = false
        do {
            _ = try await session.runRoutine(session.routines[0])
            Issue.record("run-routine without the daemon must fail")
        } catch {
            #expect(PaneSession.describe(error).contains("muxa-bridge daemon install"))
        }
    }

    @Test func approvingCreatesOneTaskAndListsIt() async throws {
        let bridge = WorkBridge()
        bridge.capabilities = ["task-records", "work:1"]
        let session = try await connected(bridge)
        let approved = try await session.approve(session.inboxEntries[0])
        #expect(approved.disposition == "approved")
        let task = try #require(approved.task)
        #expect(session.taskRecords.map(\.id) == [task])
        #expect(session.inboxEntries[0].disposition == "approved")
        // A second approval of the same entry creates nothing.
        _ = try? await session.approve(approved)
        #expect(bridge.taskRecords.count == 1)
    }

    @Test func dismissAndLaterSendTheirAction() async throws {
        let bridge = WorkBridge()
        bridge.capabilities = ["work:1"]
        let session = try await connected(bridge)
        _ = try await session.dismiss(session.inboxEntries[1])
        _ = try await session.later(session.inboxEntries[0], until: 9000)
        #expect(session.inboxEntries.map(\.disposition) == ["later", "dismissed"])
        #expect(bridge.work.contains { if case .updateInboxEntry(_, "e-00000001", .later, 9000, _, _, _) = $0 { true } else { false } })
        // A closed entry is refused by the host, and the list is read again.
        await #expect(throws: PaneSession.Failure.self) { try await session.dismiss(session.inboxEntries[1]) }
    }
}
