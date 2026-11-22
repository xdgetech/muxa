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

    @Test(.timeLimit(.minutes(1)))
    func aTasksFolderHasAReader() async throws {
        let c = try await open()
        let terminals = try await c.terminals()
        let agent = try #require(terminals.first { $0.isTaskAgent })
        let docs = try await c.readerList(agent.id, kind: .docs)
        #expect(docs.entries.map(\.path) == ["PLAN.md", ".muxa/TASK.md"])
        let (reader, plan) = try await c.readerOpen(agent.id, kind: .docs, path: "PLAN.md")
        #expect(plan.text?.hasPrefix("# Plan") == true)
        try await c.readerClose(reader)
        let changes = try await c.readerList(agent.id, kind: .changes)
        #expect(changes.entries.first?.status == "modified")
        let image = try await c.readerOpen(agent.id, kind: .images, path: "screenshots/login.png").content
        #expect(image.bytes?.prefix(4) == Data([0x89, 0x50, 0x4E, 0x47]))
        let shell = try #require(terminals.first { $0.name == "shell" })
        await #expect(throws: V2Error.self) { try await c.readerList(shell.id, kind: .docs) }
    }

    @Test(.timeLimit(.minutes(1)))
    func aLoopStartsAndStopsAndWatchersSeeIt() async throws {
        let c = try await open()
        try await c.watch()
        var events = c.events.makeAsyncIterator()
        let started = try await c.loopStart("t1", spec: LoopSpec(maxAttempts: 5))
        #expect(started.state == "armed" && started.maxAttempts == 5)
        #expect(await events.next() == .loops([started]))
        let stopped = try await c.loopStop("t1")
        #expect(stopped.state == "stopped" && stopped.reason == "user")
        #expect(try await c.loops() == [stopped])
        await #expect(throws: V2Error.self) { try await c.loopStart("t3") }
    }

    @Test(.timeLimit(.minutes(1)))
    func aTasksChatShowsItsConversationAndPrompt() async throws {
        let c = try await open()
        let (_, update) = try await c.chatOpen("t2")
        #expect(update.reset == true)
        #expect(update.entries.map(\.kind) == ["user", "assistant", "tool"])
        #expect(update.prompt?.options.map(\.label) == ["Allow", "Deny"])
        await #expect(throws: V2Error.self) { try await c.chatOpen("t3") }
    }

    @Test func anUpdateReplacesEntriesByID() {
        let before = [ChatEntry(id: "a", kind: "user", text: "1"), ChatEntry(id: "b", kind: "tool")]
        let update = ChatUpdate(entries: [ChatEntry(id: "b", kind: "tool", text: "done"), ChatEntry(id: "c", kind: "notice")])
        #expect(update.applied(to: before).map(\.id) == ["a", "b", "c"])
        #expect(update.applied(to: before)[1].text == "done")
        #expect(ChatUpdate(reset: true, entries: []).applied(to: before).isEmpty)
    }

    @Test func placesDecodeFromTheLabel() {
        let t = Terminal(
            id: "t9", name: "Audit", live: true, status: nil,
            labels: ["muxa.places": #"[{"kind":"folder","path":"/w/tasks/a"},{"kind":"read","path":"/w/docs"}]"#],
            cols: 80, rows: 24)
        #expect(t.places == [Place(kind: .folder, path: "/w/tasks/a"), Place(kind: .read, path: "/w/docs")])
    }
}
