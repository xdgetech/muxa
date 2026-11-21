import Foundation
import Testing

@testable import MuxaV2

/// The client against the in-memory demo server.
struct DemoTests {
    func open() async throws -> V2Client {
        try await V2Client.open(DemoChannel(), name: "test")
    }

    @Test(.timeLimit(.minutes(1)))
    func aTerminalShowsItsScreenAndEchoes() async throws {
        let c = try await open()
        let shell = try #require(try await c.terminals().first { $0.name == "shell" })
        var events = c.events.makeAsyncIterator()
        let ch = try await c.attach(shell.id, cols: 80, rows: 24)
        #expect(await events.next() == .paint(channel: ch))
        #expect(await events.next() == .output(channel: ch, bytes: Array("$ ".utf8)))
        try await c.input(ch, Array("ls\r".utf8))
        #expect(await events.next() == .output(channel: ch, bytes: Array("ls\r\n".utf8)))
    }

    @Test(.timeLimit(.minutes(1)))
    func aCaptureGoesOnTopAndWatchersSeeIt() async throws {
        let c = try await open()
        try await c.watch()
        var events = c.events.makeAsyncIterator()
        let capture = try await c.parseCapture("reply Sam #personal due tomorrow", today: "2026-11-25")
        #expect(capture == Capture(kind: .reply, title: "Sam", workspace: "Personal", due: "2026-11-26"))
        let inbox = try await c.inbox(.inboxAdd(capture))
        #expect(inbox.items.first?.title == "Sam")
        #expect(await events.next() == .inbox(inbox))
        let done = try await c.inbox(.inboxDone(item: inbox.items[0].id))
        #expect(done.done.first?.title == "Sam")
    }

    @Test(.timeLimit(.minutes(1)))
    func aRunningRoutineIsAConflict() async throws {
        let c = try await open()
        let (routines, _) = try await c.routines()
        #expect(routines.map(\.name) == ["dependency-digest", "nightly-backup", "release-notes"])
        try await c.runRoutine(workspace: "app", name: "release-notes")
        await #expect(throws: V2Error.refused(code: "conflict", message: "release-notes already runs")) {
            try await c.runRoutine(workspace: "app", name: "release-notes")
        }
    }

    @Test(.timeLimit(.minutes(1)))
    func aTaskStartsInItsPlaceOnce() async throws {
        let c = try await open()
        let (audit, existing) = try await c.startTask(workspace: "app", title: "Audit", places: [.read(repo: "/demo/app")])
        #expect(audit.name == "Audit" && !existing && audit.branch == nil)
        let (fix, _) = try await c.startTask(
            workspace: "app", title: "Fix", places: [.branch(repo: "/demo/app", branch: "fix-2")])
        #expect(fix.branch == "fix-2")
        let (again, running) = try await c.startTask(
            workspace: "app", title: "Fix", places: [.branch(repo: "/demo/app", branch: "fix-2")])
        #expect(running && again.id == fix.id)
    }

    @Test func placesDecodeFromTheLabel() {
        let t = Terminal(
            id: "t9", name: "Audit", live: true, status: nil,
            labels: ["muxa.places": #"[{"kind":"folder","path":"/w/tasks/a"},{"kind":"read","path":"/w/docs"}]"#],
            cols: 80, rows: 24)
        #expect(t.places == [Place(kind: .folder, path: "/w/tasks/a"), Place(kind: .read, path: "/w/docs")])
    }
}
