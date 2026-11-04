import Foundation
import MuxaProtocol
import Testing

@testable import MuxaSession

/// A bridge with Routines, stored entries and the Task's Inbox place, as the host answers them.
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
        case .updateInboxEntry(let id, let entry, let action, let until, _, _):
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
        case .updateInbox(let id, let task, let rev, let change):
            record(req)
            guard let i = taskRecords.firstIndex(where: { $0.id == task }), taskRecords[i].rev == rev else {
                return refuse(id, .conflict, "stale")
            }
            taskRecords[i].rev += 1
            taskRecords[i].inbox = TaskInbox(order: change.down ? 5000 : nil, until: change.until, since: 5)
            emit(.taskSaved(task: id, record: taskRecords[i]))
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
        // The approved entry leaves the list.
        #expect(inboxRows(queue: [], entries: session.inboxEntries, routines: [], sessions: []).map(\.id) == ["e-00000002"])
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
        #expect(bridge.work.contains { if case .updateInboxEntry(_, "e-00000001", .later, 9000, _, _) = $0 { true } else { false } })
        // A closed entry is refused by the host, and the list is read again.
        await #expect(throws: PaneSession.Failure.self) { try await session.dismiss(session.inboxEntries[1]) }
    }

    @Test func moveDownAndLaterNeedTaskInbox() async throws {
        let bridge = WorkBridge()
        bridge.capabilities = ["task-records"]
        bridge.taskRecords = [InboxTests.record("t-1")]
        let session = try await connected(bridge)
        #expect(!session.canPlaceTasks)
        await #expect(throws: PaneSession.Failure.bridge(ErrorBody(code: .unsupported, message: PaneSession.placeUnsupported))) {
            try await session.place(session.taskRecords[0], InboxChange(down: true))
        }
        #expect(bridge.work.isEmpty)
    }

    @Test func moveDownSavesThePlaceAndAStaleRevisionConflicts() async throws {
        let bridge = WorkBridge()
        bridge.capabilities = ["task-records", "task-inbox"]
        bridge.taskRecords = [InboxTests.record("t-1")]
        let session = try await connected(bridge)
        let stale = session.taskRecords[0]
        let saved = try await session.place(stale, InboxChange(down: true))
        #expect(saved.inbox?.order == 5000)
        #expect(session.taskRecords[0].rev == 2)
        await #expect(throws: PaneSession.Failure.bridge(ErrorBody(code: .conflict, message: PaneSession.taskConflict))) {
            try await session.place(stale, InboxChange(until: 9000))
        }
    }

    @Test func movedDownTasksComeAfterEntriesAndLaterHides() {
        let moved = TaskRecord(id: "t-down", rev: 1, workspace: "w", title: "d", goal: "g", status: .review,
                               createdAt: 1, updatedAt: 9, inbox: TaskInbox(order: 1, since: 200))
        let later = TaskRecord(id: "t-later", rev: 1, workspace: "w", title: "l", goal: "g", status: .review,
                               createdAt: 1, updatedAt: 9, inbox: TaskInbox(until: 500, since: 200))
        let plain = InboxTests.record("t-plain", .review)
        let sessions = [InboxTests.session("t-down", .idle), InboxTests.session("t-later", .idle),
                        InboxTests.session("t-plain", .idle), InboxTests.session("t-run", .waiting)]
        let queue = [moved, later, plain].triageQueue(sessions: sessions, now: 300)
        let routines = [RoutineInfo(workspace: "w", name: "r", enabled: true,
                                    running: RoutineRun(occurrence: "now-1", startedAt: 1, task: "t-run"))]
        let entries = [Self.entry("e-1", kind: "briefing"), Self.entry("e-2", kind: "proposal", disposition: "dismissed"),
                       Self.entry("e-3", kind: "proposal", disposition: "later", until: 400),
                       Self.entry("e-4", kind: "briefing", disposition: "later", until: 200)]
        let rows = inboxRows(queue: queue, entries: entries, routines: routines, sessions: sessions, now: 300)
        #expect(rows.map(\.id) == ["t-plain", "t-run", "e-1", "e-4", "t-down"])
    }

    @Test func aRoutineRunWithoutAWaitingAgentIsNoNeed() {
        let routines = [RoutineInfo(workspace: "w", name: "r", enabled: true,
                                    running: RoutineRun(occurrence: "now-1", startedAt: 1, task: "t-run"))]
        #expect(routines.needs(sessions: [InboxTests.session("t-run", .running)]).isEmpty)
        #expect(routines.needs(sessions: [InboxTests.session("t-run", .waiting, shellForeground: true)]).isEmpty)
        #expect(routines.needs(sessions: [InboxTests.session("t-run", .waiting)]).map(\.task) == ["t-run"])
    }

    @Test func laterTimesAreInTheFuture() throws {
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = try #require(TimeZone(identifier: "Asia/Tokyo"))
        // 2026-11-03 23:30 in Tokyo.
        let now = Date(timeIntervalSince1970: 1_762_180_200)
        #expect(LaterChoice.inAnHour.until(now: now, calendar: calendar) == 1_762_183_800)
        // 2026-11-04 09:00 in Tokyo.
        #expect(LaterChoice.tomorrowMorning.until(now: now, calendar: calendar) == 1_762_214_400)
        #expect(laterProblem(now, now: now) != nil)
        #expect(laterProblem(now.addingTimeInterval(400 * 86_400), now: now) != nil)
        #expect(laterProblem(now.addingTimeInterval(60), now: now) == nil)
    }
}
