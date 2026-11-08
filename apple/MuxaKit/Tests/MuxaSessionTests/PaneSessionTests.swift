import Foundation
import MuxaProtocol
import MuxaTransport
import Testing

@testable import MuxaSession

/// An in-memory bridge: answers every request, and records them in order.
class FakeBridge: BridgeChannel, @unchecked Sendable {
    let lines: AsyncThrowingStream<Data, any Error>
    private let out: AsyncThrowingStream<Data, any Error>.Continuation
    private let lock = NSLock()
    private var _received: [Request] = []
    private var _closed = false
    var protocolVersion: UInt32 = 1
    var sessions: [Session] = [
        Session(id: "$1", name: "demo", panes: [
            Pane(id: "%1", window: "@1", index: 0, role: .agent, cols: 20, rows: 5,
                 command: "claude", path: "/w", dead: false, shellForeground: false),
        ])
    ]
    var capabilities: [String] = []
    var docs: [DocInfo] = []
    var changes: [ChangeInfo] = []
    var images: [ImageInfo] = []
    var taskRecords: [TaskRecord] = []
    var workers: [WorkerInfo] = []
    var workspaces: [WorkspaceInfo] = [WorkspaceInfo(name: "work")]
    /// Tasks whose worktree has unmerged work: `update-task` with `remove` is refused (ADR 0043).
    var unmerged: Set<String> = []
    /// Tasks whose agent is running: `restart-agent` without force is refused.
    var runningAgents: Set<String> = []
    /// False: the daemon went away after `hello`, so loop requests are `unsupported`.
    var daemon = true
    /// Panes whose `chat` is answered with `unsupported`.
    var unsupportedChats: Set<String> = []
    /// Screen data sent after each successful watch.
    var screen = Data("\u{1b}chello".utf8)

    init() {
        (lines, out) = AsyncThrowingStream<Data, any Error>.makeStream()
    }

    var received: [Request] { lock.withLock { _received } }
    var closed: Bool { lock.withLock { _closed } }

    func start() { emit(.hello(protocol: protocolVersion, bridge: "test", tmux: "3.7c", capabilities: capabilities)) }

    func emit(_ message: Message) { out.yield(try! Wire.encode(message)) }

    func send(_ line: Data) async throws {
        let req = try Wire.decodeRequest(line)
        lock.withLock { _received.append(req) }
        switch req {
        case .list(let id): emit(.result(id: id, error: nil, sessions: sessions))
        case .watch(let id, let pane):
            emit(.result(id: id, error: nil, sessions: nil))
            emit(.screen(watch: id, pane: pane, cols: 20, rows: 5, data: screen))
        case .history(let id, let pane, let lines):
            let text = (1...(lines ?? 3)).map { "line \($0)\n" }.joined()
            emit(.history(history: id, pane: pane, cols: 20, data: Data(text.utf8), truncated: lines != nil))
            emit(.result(id: id, error: nil, sessions: nil))
        case .docs(let id, _):
            emit(.docList(DocListing(docs: id, root: "app", entries: docs)))
            emit(.result(id: id, error: nil, sessions: nil))
        case .changes(let id, _):
            emit(.changeList(ChangeListing(changes: id, root: "app", entries: changes)))
            emit(.result(id: id, error: nil, sessions: nil))
        case .images(let id, _):
            emit(.imageList(ImageListing(images: id, root: "app", entries: images)))
            emit(.result(id: id, error: nil, sessions: nil))
        case .newTask(let id, _, let branch, _):
            if branch.hasPrefix("-") {
                emit(.result(id: id, error: ErrorBody(code: .badRequest, message: "bad branch"), sessions: nil))
                return
            }
            let name = branch.replacingOccurrences(of: "/", with: "-")
            sessions.append(Session(id: "$9", name: name, panes: []))
            emit(.taskCreated(task: id, session: name))
            emit(.result(id: id, error: nil, sessions: nil))
        case .listTasks(let id):
            emit(.taskList(TaskListing(tasks: id, workspaces: workspaces, entries: taskRecords, workers: workers)))
            emit(.result(id: id, error: nil, sessions: nil))
        case .createTask(let id, let workspace, let title, let goal, _, _, let branch, _, let loop, _):
            guard workspaces.contains(where: { $0.name == workspace }) else {
                emit(.result(id: id, error: ErrorBody(code: .badRequest, message: "no such workspace"), sessions: nil))
                return
            }
            let launched = branch != "taken"
            let record = TaskRecord(
                id: "t-\(id)", rev: 1, workspace: workspace, title: title, goal: goal, status: .open,
                createdAt: 1, updatedAt: 1, note: launched ? nil : "the branch exists",
                loop: loop.map { Self.armed($0, attempts: 0) })
            taskRecords.append(record)
            emit(.taskSaved(task: id, record: record))
            emit(.result(id: id, error: launched ? nil : ErrorBody(code: .taskFailed, message: "launch failed"), sessions: nil))
        case .updateTask(let id, let task, let rev, let status, let worktree):
            guard let i = taskRecords.firstIndex(where: { $0.id == task }), taskRecords[i].rev == rev else {
                emit(.result(id: id, error: ErrorBody(code: .conflict, message: "stale"), sessions: nil))
                return
            }
            if worktree == .remove, unmerged.contains(task) {
                emit(.result(id: id, error: ErrorBody(code: .wouldLoseWork, message: "its branch is not merged"), sessions: nil))
                return
            }
            taskRecords[i].rev += 1
            taskRecords[i].status = status
            emit(.taskSaved(task: id, record: taskRecords[i]))
            emit(.result(id: id, error: nil, sessions: nil))
        case .recoverRestart(let id, let task, let rev, let operation, _):
            guard let i = taskRecords.firstIndex(where: { $0.id == task }), taskRecords[i].rev == rev,
                  taskRecords[i].restartOperation?.id == operation else {
                emit(.result(id: id, error: ErrorBody(code: .conflict, message: "task changed"), sessions: nil))
                return
            }
            taskRecords[i].rev += 1
            taskRecords[i].restartOperation = nil
            emit(.taskSaved(task: id, record: taskRecords[i]))
            emit(.result(id: id, error: nil, sessions: nil))
        case .restartAgent(let id, let task, let force, _, _):
            guard let i = taskRecords.firstIndex(where: { $0.id == task }) else {
                emit(.result(id: id, error: ErrorBody(code: .badRequest, message: "\(task) is not a task"), sessions: nil))
                return
            }
            guard force || !runningAgents.contains(task) else {
                emit(.result(id: id, error: ErrorBody(code: .badRequest, message: "the agent is running"), sessions: nil))
                return
            }
            let next = (taskRecords[i].runs.last?.id ?? 0) + 1
            if !taskRecords[i].runs.isEmpty { taskRecords[i].runs[taskRecords[i].runs.count - 1].endedAt = 9 }
            taskRecords[i].runs.append(AgentRun(id: next, startedAt: 9, reason: .restart))
            taskRecords[i].rev += 1
            emit(.taskSaved(task: id, record: taskRecords[i]))
            emit(.result(id: id, error: nil, sessions: nil))
        case .startLoop(let id, let task, let loop, let reset):
            changeLoop(id, task) { current in
                guard loop.check == "make check" else { return nil }
                return Self.armed(loop, attempts: reset ? 0 : current?.attempts ?? 0)
            }
        case .stopLoop(let id, let task):
            changeLoop(id, task) { current in
                var stopped = current
                stopped?.state = .stopped
                stopped?.reason = .user
                return stopped
            }
        case .chat(let id, let pane) where unsupportedChats.contains(pane):
            emit(.result(id: id, error: ErrorBody(code: .unsupported, message: "no adapter"), sessions: nil))
        case .bye: out.finish()
        default: emit(.result(id: req.id, error: nil, sessions: nil))
        }
    }

    static func armed(_ spec: LoopSpec, attempts: UInt32) -> TaskLoop {
        TaskLoop(check: spec.check, maxAttempts: spec.maxAttempts ?? 3, timeout: spec.timeout ?? 600, state: .armed,
                 attempts: attempts, armedAt: 9, since: 9)
    }

    /// Saves the loop `change` returns; nil refuses the request as the host would.
    private func changeLoop(_ id: UInt64, _ task: String, _ change: (TaskLoop?) -> TaskLoop?) {
        guard daemon else {
            emit(.result(id: id, error: ErrorBody(code: .unsupported, message: loopNeedsDaemon), sessions: nil))
            return
        }
        guard let i = taskRecords.firstIndex(where: { $0.id == task }), let loop = change(taskRecords[i].loop) else {
            emit(.result(id: id, error: ErrorBody(code: .badRequest, message: "not allowed"), sessions: nil))
            return
        }
        taskRecords[i].loop = loop
        taskRecords[i].rev += 1
        emit(.taskSaved(task: id, record: taskRecords[i]))
        emit(.result(id: id, error: nil, sessions: nil))
    }

    func close() async {
        lock.withLock { _closed = true }
        out.finish()
    }
}

@MainActor
struct PaneSessionTests {
    func connected(_ bridge: FakeBridge = FakeBridge()) async throws -> (PaneSession, HeadlessScreen, FakeBridge) {
        let session = PaneSession()
        let screen = HeadlessScreen()
        session.renderer = screen
        try await session.connect {
            bridge.start()
            return bridge
        }
        return (session, screen, bridge)
    }

    @Test func aNewTaskReturnsItsSessionAfterListingAgain() async throws {
        let bridge = FakeBridge()
        bridge.capabilities = ["tasks"]
        let (session, _, _) = try await connected(bridge)
        #expect(session.canStartTasks)
        let name = try await session.newTask(repo: "/w/app", branch: "feat/x", base: nil)
        #expect(name == "feat-x")
        #expect(session.sessions.map(\.name).contains("feat-x"))
        #expect(bridge.received.contains {
            if case .newTask(_, "/w/app", "feat/x", nil) = $0 { true } else { false }
        })

        await #expect(throws: PaneSession.Failure.self) {
            try await session.newTask(repo: "/w/app", branch: "-x", base: nil)
        }
    }

    @Test func connectReadsHelloAndLists() async throws {
        let (session, _, _) = try await connected()
        #expect(session.phase == .connected)
        #expect(session.tmuxVersion == "3.7c")
        #expect(session.sessions.map(\.name) == ["demo"])
    }

    @Test func aBridgeThatExitsBeforeHelloSaysItDidNotStart() async throws {
        let bridge = FakeBridge()
        let session = PaneSession()
        await #expect(throws: PaneSession.Failure.noHello) {
            try await session.connect {
                Task { await bridge.close() }
                return bridge
            }
        }
        #expect(session.phase == .disconnected(reason: PaneSession.describe(PaneSession.Failure.noHello)))
    }

    @Test func aProtocolMismatchFailsTheConnect() async throws {
        let bridge = FakeBridge()
        bridge.protocolVersion = 2
        let session = PaneSession()
        await #expect(throws: PaneSession.Failure.protocolMismatch(bridge: 2)) {
            try await session.connect {
                bridge.start()
                return bridge
            }
        }
        guard case .disconnected = session.phase else {
            Issue.record("expected disconnected, got \(session.phase)")
            return
        }
        #expect(bridge.closed)
    }

    @Test func watchRendersTheScreenAndDropsStaleWatches() async throws {
        let (session, screen, bridge) = try await connected()
        try await session.watch(pane: "%1")
        try await waitUntil { screen.rows == ["hello"] }

        // Output for an older watch must not reach the renderer.
        bridge.emit(.output(watch: 999, pane: "%1", data: Data(" stale".utf8)))
        guard case .watch(let watch, _) = bridge.received.last else {
            Issue.record("no watch sent")
            return
        }
        bridge.emit(.output(watch: watch, pane: "%1", data: Data(" world".utf8)))
        try await waitUntil { screen.rows == ["hello world"] }
    }

    @Test func inputKeepsItsOrder() async throws {
        let (session, _, bridge) = try await connected()
        try await session.watch(pane: "%1")
        session.send(text: "a")
        session.send(keys: ["Enter"])
        session.send(bytes: Data([0x1b]))
        session.paste("p")
        try await waitUntil { bridge.received.count == 6 }
        let tail = bridge.received.suffix(4).map { req -> String in
            switch req {
            case .text(_, "%1", let t): "text \(t)"
            case .keys(_, "%1", let k): "keys \(k.joined())"
            case .bytes(_, "%1", let d): "bytes \(d.count)"
            case .paste(_, "%1", let t): "paste \(t)"
            default: "other"
            }
        }
        #expect(tail == ["text a", "keys Enter", "bytes 1", "paste p"])
    }

    @Test func paneGoneIsReported() async throws {
        let (session, _, bridge) = try await connected()
        try await session.watch(pane: "%1")
        guard case .watch(let watch, _) = bridge.received.last else { return }
        bridge.emit(.paneGone(watch: watch, pane: "%1", reason: nil))
        try await waitUntil { session.paneGoneReason != nil }
    }

    @Test func byeFromTheBridgeDisconnects() async throws {
        let (session, _, bridge) = try await connected()
        bridge.emit(.bye(reason: "idle timeout"))
        try await waitUntil { session.phase == .disconnected(reason: "idle timeout") }
    }

    @Test func disconnectSendsBye() async throws {
        let (session, _, bridge) = try await connected()
        await session.disconnect()
        try await waitUntil {
            if case .bye = bridge.received.last { return true }
            return false
        }
        #expect(bridge.closed)
    }

    @Test func reconnectWatchesTheSamePane() async throws {
        let (session, screen, first) = try await connected()
        try await session.watch(pane: "%1")
        await first.close()
        try await waitUntil {
            if case .disconnected = session.phase { return true }
            return false
        }

        let second = FakeBridge()
        second.screen = Data("\u{1b}cagain".utf8)
        try await session.connect {
            second.start()
            return second
        }
        try await waitUntil { screen.rows == ["again"] }
        #expect(session.watchedPane == "%1")
    }

    @Test func aFailedInputIsShown() async throws {
        let bridge = FailingInput()
        let (session, _, _) = try await connected(bridge)
        try await session.watch(pane: "%1")
        session.send(keys: ["Nope"])
        try await waitUntil { session.lastError == "unsupported key Nope" }
    }
}

/// Rejects every `keys` request.
final class FailingInput: FakeBridge, @unchecked Sendable {
    override func send(_ line: Data) async throws {
        if case .keys(let id, _, let keys) = try Wire.decodeRequest(line) {
            emit(.result(id: id, error: ErrorBody(code: .unsupportedKey, message: "unsupported key \(keys[0])"), sessions: nil))
            return
        }
        try await super.send(line)
    }
}

@MainActor
func waitUntil(_ condition: () -> Bool, timeout: Duration = .seconds(5)) async throws {
    let deadline = ContinuousClock.now + timeout
    while !condition() {
        if ContinuousClock.now > deadline {
            Issue.record("condition not met in \(timeout)")
            return
        }
        try await Task.sleep(for: .milliseconds(10))
    }
}
