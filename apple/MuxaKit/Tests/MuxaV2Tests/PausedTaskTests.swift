import Foundation
import MuxaTransport
import Testing

@testable import MuxaV2

/// Pause and resume a Task (ADR 0102): the wire, and the demo server.
struct PausedTaskTests {
    func open() async throws -> V2Client {
        try await V2Client.open(DemoChannel(), name: "test")
    }

    @Test func theRequestsMatchTheSpec() throws {
        func wire(_ r: V2Request) throws -> String { String(decoding: try r.json(id: 1), as: UTF8.self) }
        #expect(try wire(.pauseTask(terminal: "t1")) == #"{"id":1,"terminal":"t1","type":"pause-task"}"#)
        #expect(try wire(.resumeTask(terminal: "t1")) == #"{"id":1,"terminal":"t1","type":"resume-task"}"#)
        #expect(try wire(.pausedTasks) == #"{"id":1,"type":"paused-tasks"}"#)
    }

    @Test func aPausedTaskDecodesWithItsLoop() throws {
        let wire = #"{"paused":[{"terminal":{"id":"t4","name":"fix","live":false,"status":null,"labels":{"muxa.kind":"task","muxa.role":"agent","muxa.title":"Fix it"},"cols":80,"rows":24},"paused_at":1764000000,"loop":{"check":"make test","max_attempts":5}},{"terminal":{"id":"t5","name":null,"live":false,"status":null,"labels":{},"cols":80,"rows":24},"paused_at":7}]}"#
        let paused: [Paused] = try field("paused", of: Data(wire.utf8))
        #expect(paused.map(\.id) == ["t4", "t5"])
        #expect(paused[0].terminal.taskTitle == "Fix it" && !paused[0].terminal.live)
        #expect(paused[0].pausedAt == 1_764_000_000)
        #expect(paused[0].loop == LoopSpec(check: "make test", maxAttempts: 5))
        #expect(paused[1].loop == nil)
    }

    @Test(.timeLimit(.minutes(1)))
    func anOlderServerTimesOutAndForgetsTheRequest() async throws {
        let c = try await V2Client.open(OlderServer(), name: "test")
        do {
            _ = try await c.pausedTasks(timeout: .milliseconds(100))
            Issue.record("an older server never answers")
        } catch V2Error.timedOut {}
        #expect(await c.pendingRequests == 0)
    }

    @Test(.timeLimit(.minutes(1)))
    func theDemoShipsOnePausedTask() async throws {
        let c = try await open()
        let paused = try await c.pausedTasks()
        #expect(paused.map(\.terminal.taskTitle) == ["Wait for the API review"])
        #expect(try await !c.terminals().contains { $0.id == paused[0].id }, "a paused Task runs no terminal")
    }

    @Test(.timeLimit(.minutes(1)))
    func pauseEndsTheAgentAndItsShellsAndKeepsTheTaskAndItsItems() async throws {
        let c = try await open()
        _ = try await c.loopStart("t1", spec: LoopSpec(check: "make lint"))
        try await c.watch()
        var events = c.events.makeAsyncIterator()
        let paused = try await c.pauseTask("t1")
        #expect(paused.id == "t1" && !paused.terminal.live && paused.terminal.status == nil)
        #expect(paused.terminal.branch == "fix-login")
        #expect(paused.loop?.check == "make lint")
        guard case .loops(let loops) = await events.next() else { Issue.record("no loops event"); return }
        #expect(loops.first { $0.terminal == "t1" }?.reason == "user", "the loop stops")
        #expect(await events.next() == .removed(terminal: "t1"))
        #expect(await events.next() == .removed(terminal: "t6"), "the Task's shell ends with it")
        guard case .paused(let all) = await events.next() else { Issue.record("no paused event"); return }
        #expect(all.map(\.id) == ["t10", "t1"])
        #expect(try await c.inbox().items.first { $0.id == "d1" }?.terminal == "t1", "the Review item stays")
        await #expect(throws: V2Error.self) { try await c.pauseTask("t3") }
    }

    @Test(.timeLimit(.minutes(1)))
    func resumeRunsTheAgentAgainAndMovesItsItems() async throws {
        let c = try await open()
        _ = try await c.loopStart("t1", spec: LoopSpec(check: "make lint"))
        _ = try await c.pauseTask("t1")
        let (t, existing) = try await c.resumeTask("t1")
        #expect(!existing && t.live && t.id != "t1")
        #expect(t.taskDir == "/demo/app-fix-login" && t.branch == "fix-login")
        #expect(try await c.pausedTasks().map(\.id) == ["t10"])
        #expect(try await c.inbox().items.first { $0.id == "d1" }?.terminal == t.id)
        #expect(try await c.loops().first { $0.terminal == t.id }?.state == "armed", "the loop is armed again")
        do {
            _ = try await c.resumeTask("t1")
            Issue.record("resume of a Task that is not paused")
        } catch let V2Error.refused(code, _) {
            #expect(code == "not-found")
        }
    }

    @Test(.timeLimit(.minutes(1)))
    func closeTaskClosesAPausedTask() async throws {
        let c = try await open()
        _ = try await c.pauseTask("t1")
        let inbox = try await c.inbox(.closeTask(terminal: "t1"))
        #expect(inbox.done.first?.id == "d1", "its Review item is Done")
        #expect(try await c.pausedTasks().map(\.id) == ["t10"])
    }

    @Test(.timeLimit(.minutes(1)))
    func aTaskStartedInAPausedPlaceEndsThePause() async throws {
        let c = try await open()
        let (t, _) = try await c.startTask(
            workspace: "app", title: "API review", places: [.existing(path: "/demo/app-api-review")])
        #expect(t.id != "t10", "a new id, not the paused Task's")
        #expect(try await c.pausedTasks().isEmpty)
    }
}

/// A server before ADR 0102: welcome, then an error without an id for every
/// request it does not know, as the daemon answers one.
private final class OlderServer: ByteChannel, @unchecked Sendable {
    let bytes: AsyncThrowingStream<Data, any Error>
    private let out: AsyncThrowingStream<Data, any Error>.Continuation

    init() { (bytes, out) = AsyncThrowingStream.makeStream() }

    func send(_ data: Data) async throws {
        var buf = Array(data)
        while let frame = try Frame.decode(&buf) {
            let msg = try JSONSerialization.jsonObject(with: Data(frame.payload)) as? [String: Any]
            let answer: [String: Any] = msg?["type"] as? String == "hello"
                ? ["type": "welcome", "version": 2, "token": "old", "resumed": []]
                : ["type": "error", "code": "bad-request", "message": "unknown request"]
            out.yield(Data(Frame.control(try JSONSerialization.data(withJSONObject: answer)).encode()))
        }
    }

    func close() async { out.finish() }
}
