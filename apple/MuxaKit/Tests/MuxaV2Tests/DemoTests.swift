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
    func turnedOffRoutinesAreReportedUntilTurnedOn() async throws {
        let c = try await open()
        #expect(try await c.routines().off == [RoutinesOff(workspace: "Personal", count: 1)])
        let settings = try await c.settings()
        _ = try await c.settingsUpdate(revision: settings.revision,
                                       ops: [.routinesGate(workspace: "Personal", enabled: true)])
        #expect(try await c.routines().off.isEmpty)
    }

    @Test(.timeLimit(.minutes(1)))
    func aBotsItemsCarryTheirLinksAndProposal() async throws {
        let c = try await open()
        let items = try await c.inbox().items.filter { $0.bot == "pr-watch" }
        // d12 is pr-watch's settings proposal from a talk (ADR 0083).
        #expect(items.map(\.id) == ["d10", "d11", "d12"])
        #expect(items[0].proposal?.target == "example/app#57")
        #expect(items[1].links?.first?.url.hasPrefix("https://") == true)
        let scan = try #require(try await c.routines().routines.first { $0.name == "scan" })
        #expect(scan.bot == "pr-watch")
    }

    @Test(.timeLimit(.minutes(1)))
    func aRunningRoutineIsAConflict() async throws {
        let c = try await open()
        let routines = try await c.routines().routines
        #expect(routines.map(\.name) == ["dependency-digest", "nightly-backup", "release-notes", "scan"])
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
    func aMigratedTaskResumesInItsPlace() async throws {
        let c = try await open()
        let item = try #require(try await c.inbox().items.first { $0.place != nil })
        let place = try #require(item.place)
        let (t, existing) = try await c.startTask(
            workspace: place.workspace, title: "Tidy the release notes", places: [.existing(path: place.path)])
        #expect(t.name == "Tidy the release notes" && !existing && t.taskDir == place.path)
        let (again, running) = try await c.startTask(
            workspace: place.workspace, title: "Tidy the release notes", places: [.existing(path: place.path)])
        #expect(running && again.id == t.id)
        await #expect(throws: V2Error.self) {
            try await c.startTask(workspace: "app", title: "X", places: [.existing(path: place.path), .folder])
        }
    }

    @Test(.timeLimit(.minutes(1)))
    func aCheckoutRunsOneAgentInTheReposOwnFolder() async throws {
        let c = try await open()
        let (t, existing) = try await c.startTask(workspace: "app", title: "app main", places: [.checkout(repo: "app")])
        #expect(!existing && t.taskDir == "/demo/app" && t.labels["muxa.checkout"] == "app")
        let (again, running) = try await c.startTask(workspace: "app", title: "app main", places: [.checkout(repo: "app")])
        #expect(running && again.id == t.id)
    }

    @Test(.timeLimit(.minutes(1)))
    func aTaskStartsWithItsLoopAndGetsANewID() async throws {
        let c = try await open()
        let review = try #require(try await c.inbox().items.first { $0.kind == .review })
        _ = try await c.inbox(.inboxApprove(item: review.id))
        let (t, _) = try await c.startTask(
            workspace: "app", title: "Tests", loop: LoopSpec(check: "make lint", maxAttempts: 2))
        // Approve closed one of three terminals: the new one must not take the shell's id.
        #expect(try await c.terminals().filter { $0.id == t.id }.count == 1)
        let loops = try await c.loops()
        #expect(loops.map(\.terminal) == [t.id])
        #expect(loops.first?.check == "make lint" && loops.first?.maxAttempts == 2)
        await #expect(throws: V2Error.self) {
            try await c.startTask(workspace: "app", title: "X", loop: LoopSpec(check: "rm -rf /"))
        }
        await #expect(throws: V2Error.self) {
            try await c.startTask(workspace: "Personal", title: "X", loop: LoopSpec())
        }
        await #expect(throws: V2Error.self) { try await c.startTask(workspace: "app", title: " ") }
    }

    @Test(.timeLimit(.minutes(1)))
    func thePlanLinksTheTasksBrief() async throws {
        let c = try await open()
        let docs = try await c.readerList("t1", kind: .docs)
        let plan = try await c.readerOpen("t1", kind: .docs, path: "PLAN.md").content
        let href = try #require(plan.text?.firstMatch(of: /\]\(([^)]+)\)/)).1
        #expect(ReaderLink.document(String(href), from: plan.path, in: docs.entries) == ".muxa/TASK.md")
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

    @Test(.timeLimit(.minutes(1)))
    func workspacesListTheirReposChecksAndConnections() async throws {
        let c = try await open()
        let workspaces = try await c.workspaces()
        #expect(workspaces.map(\.name) == ["app", "Personal"])
        #expect(workspaces[0].repos == ["app", "docs"])
        #expect(workspaces[0].loop?.checks == .list(["make test", "make lint"]))
        #expect(workspaces[0].connections.filter(\.isDefault).map(\.name) == ["github", "linear"])
        #expect(workspaces[0].mcp.map(\.name) == ["docs"])
        #expect(workspaces[1].loop == nil && workspaces[1].connections.isEmpty)
    }

    @Test(.timeLimit(.minutes(1)))
    func approveClosesTheTaskAndWatchersSeeIt() async throws {
        let c = try await open()
        try await c.watch()
        var events = c.events.makeAsyncIterator()
        let inbox = try await c.inbox(.inboxApprove(item: "d1"))
        #expect(inbox.done.first?.id == "d1")
        #expect(!inbox.items.contains { $0.id == "d1" })
        #expect(await events.next() == .removed(terminal: "t1"))
        #expect(await events.next() == .removed(terminal: "t6"), "the Task's shell goes with it")
        #expect(await events.next() == .inbox(inbox))
        // t7 is pr-watch's talk, t8 its scan run, t9 scout's talk (ADR 0086).
        #expect(try await c.terminals().map(\.id) == ["t2", "t3", "t4", "t5", "t7", "t8", "t9"])
        await #expect(throws: V2Error.self) { try await c.inbox(.inboxApprove(item: "d2")) }
        do {
            _ = try await c.inbox(.inboxApprove(item: "nope"))
            Issue.record("approve on a missing item")
        } catch let V2Error.refused(code, _) {
            #expect(code == "not-found")
        }
    }

    @Test(.timeLimit(.minutes(1)))
    func aTasksShellsStartAndCloseAndCloseTaskTakesThemAll() async throws {
        let c = try await open()
        let shell = try await c.taskShell("t1")
        #expect(shell.isTaskShell && shell.taskDir == "/demo/app-fix-login")
        _ = try await c.taskShell("t1")
        do {
            _ = try await c.taskShell("t1")
            Issue.record("three shells per Task")
        } catch let V2Error.refused(code, _) {
            #expect(code == "conflict")
        }
        await #expect(throws: V2Error.self) { try await c.closeTerminal("t1") }
        try await c.closeTerminal(shell.id)
        #expect(try await !c.terminals().contains { $0.id == shell.id })
        let inbox = try await c.inbox(.closeTask(terminal: "t1"))
        #expect(inbox.done.first?.id == "d1", "its Review item is Done")
        #expect(try await c.terminals().allSatisfy { $0.taskDir != "/demo/app-fix-login" })
    }

    @Test(.timeLimit(.minutes(1)))
    func anUploadGoesToTheTasksUploadsFolder() async throws {
        let c = try await open()
        #expect(try await c.upload(Data("log".utf8), name: "build.log", to: "t1") == ".muxa/uploads/build.log")
        #expect(try await c.upload(Data("log".utf8), name: "build.log", to: "t1") == ".muxa/uploads/build 2.log")
        await #expect(throws: V2Error.self) { try await c.upload(Data(), name: "../x", to: "t1") }
        await #expect(throws: V2Error.self) { try await c.upload(Data(), name: "x", to: "t4") }
    }

    @Test(.timeLimit(.minutes(1)))
    func approveAsksForForceWhenTheWorktreeHoldsWork() async throws {
        let c = try await open()
        do {
            _ = try await c.inbox(.inboxApprove(item: "d8"))
            Issue.record("d8's worktree holds work")
        } catch let V2Error.refused(code, message) {
            #expect(code == "would-lose-work")
            #expect(message.contains("uncommitted changes"))
        }
        #expect(try await c.terminals().contains { $0.id == "t2" }, "nothing closed")
        #expect(try await c.inbox().items.contains { $0.id == "d8" })
        let forced = try await c.inbox(.inboxApprove(item: "d8", worktree: .force))
        #expect(forced.done.first?.id == "d8")
        #expect(try await !c.terminals().contains { $0.id == "t2" })
    }

    @Test(.timeLimit(.minutes(1)))
    func workspacesSayWhoUsesEachConnectionAndServer() async throws {
        let c = try await open()
        let app = try #require(try await c.workspaces().first)
        // t7 and t9, the Bots' talks, are Task agents too (ADR 0083).
        #expect(app.connections[0].usedBy == UsedBy(
            routines: ["dependency-digest", "nightly-backup", "release-notes"], tasks: ["t1", "t2", "t7", "t9"]))
        #expect(app.connections[0].lastUsed == 1_764_000_000)
        #expect(app.connections[1].usedBy == UsedBy(
            routines: ["dependency-digest", "nightly-backup", "release-notes"], tasks: ["t1", "t2", "t7", "t9"]))
        // A named connection is ignored: every agent gets all of them (ADR 0075).
        let (t, _) = try await c.startTask(workspace: "app", title: "Triage", connections: ["linear"])
        #expect(t.labels["muxa.connections"] == #"["github","linear"]"#)
        #expect(t.labels["muxa.mcp"] == #"["docs"]"#)
        let after = try #require(try await c.workspaces().first)
        #expect(after.connections[1].usedBy.tasks == ["t1", "t2", "t7", "t9", t.id])
        #expect(after.mcp[0].usedBy.tasks == ["t1", "t2", "t7", "t9", t.id])
    }

    @Test(.timeLimit(.minutes(1)))
    func requestChangesTypesTheCommentIntoTheAgent() async throws {
        let c = try await open()
        var events = c.events.makeAsyncIterator()
        let ch = try await c.attach("t1", cols: 80, rows: 24)
        _ = await events.next()
        _ = await events.next()
        await #expect(throws: V2Error.refused(code: "bad-request", message: "the comment is empty")) {
            try await c.inbox(.inboxRequestChanges(item: "d1", comment: " "))
        }
        let inbox = try await c.inbox(.inboxRequestChanges(item: "d1", comment: "Add a test"))
        #expect(inbox.done.first?.id == "d1")
        #expect(await events.next() == .output(channel: ch, bytes: Array("Add a test\r\n".utf8)))
        #expect(try await c.terminals().contains { $0.id == "t1" })
    }

    @Test(.timeLimit(.minutes(1)))
    func retryRunsWhatFailedAgain() async throws {
        let c = try await open()
        let routine = try await c.inbox(.inboxRetry(item: "d5"))
        #expect(routine.done.first?.id == "d5")
        let routines = try await c.routines().routines
        #expect(routines.first { $0.name == "nightly-backup" }?.runningSince != nil)
        let task = try await c.inbox(.inboxRetry(item: "d7"))
        #expect(task.done.first?.id == "d7")
        #expect(try await c.terminals().first { $0.id == "t2" }?.status?.state == "working")
        do {
            _ = try await c.inbox(.inboxRetry(item: "d1"))
            Issue.record("retry on a Review item")
        } catch let V2Error.refused(code, _) {
            #expect(code == "bad-request")
        }
    }

    @Test(.timeLimit(.minutes(1)))
    func settingsShowTheWorkspacesWithoutSecrets() async throws {
        let c = try await open()
        let s = try await c.settings()
        #expect(s.revision == "1" && !s.readOnly && s.problem == nil)
        #expect(s.workspaces.map(\.name) == ["app", "Personal"])
        let app = s.workspaces[0]
        #expect(app.connections.map(\.secret) == [.set, .missing])
        #expect(app.mcp.map(\.name) == ["docs"])
        #expect(app.routines.first?.instructionsText?.isEmpty == false)
        #expect(s.workers.map(\.name) == ["reviewer"])
    }

    @Test(.timeLimit(.minutes(1)))
    func settingsUpdateAppliesTheOpsInOrder() async throws {
        let c = try await open()
        let after = try await c.settingsUpdate(revision: "1", ops: [
            .envSet(workspace: "app", key: "AAA", value: "v4lue"),
            .connectionAdd(workspace: "app", connection: ConnectionSpec(name: "sentry", env: "SENTRY_TOKEN"), secret: "s3cret"),
            .connectionAdd(workspace: "Personal", connection: ConnectionSpec(name: "mail", env: "MAIL_TOKEN")),
            .workspaceUpdate(workspace: "app", agentCommand: .clear, parkAfter: .set("12h")),
            .routineUpdate(workspace: "app", routine: RoutineSpec(name: "dependency-digest", instructions: "routines/d.md"),
                           instructionsText: "New text.\n"),
            .workspaceRemove(workspace: "Personal"),
        ])
        #expect(after.revision == "2")
        let app = try #require(after.workspaces.first)
        #expect(after.workspaces.map(\.name) == ["app"])
        #expect(app.agent.env == [EnvKey(key: "AAA"), EnvKey(key: "CLAUDE_CONFIG_DIR")])
        #expect(app.agent.command == nil && app.parkAfter == "12h")
        #expect(app.connections.first { $0.name == "sentry" }?.secret == .set)
        #expect(app.routines[0].instructionsText == "New text.\n")
        let raw = String(decoding: try Coding.plainEncoder.encode(after), as: UTF8.self)
        #expect(!raw.contains("s3cret") && !raw.contains("v4lue"))
        #expect(try await c.settings() == after)
    }

    @Test(.timeLimit(.minutes(1)))
    func aStaleRevisionIsAConflict() async throws {
        let c = try await open()
        _ = try await c.settingsUpdate(revision: "1", ops: [.envUnset(workspace: "app", key: "CLAUDE_CONFIG_DIR")])
        do {
            _ = try await c.settingsUpdate(revision: "1", ops: [.workspaceRemove(workspace: "app")])
            Issue.record("a stale revision applied")
        } catch let V2Error.refused(code, _) {
            #expect(code == "conflict")
        }
        #expect(try await c.settings().workspaces.map(\.name) == ["app", "Personal"])
    }

    @Test(.timeLimit(.minutes(1)))
    func aFailedOpChangesNothing() async throws {
        let c = try await open()
        let before = try await c.settings()
        do {
            _ = try await c.settingsUpdate(revision: "1", ops: [
                .envSet(workspace: "app", key: "AAA", value: "v"),
                .connectionSecret(workspace: "app", name: "nope", secret: "s3cret"),
            ])
            Issue.record("an unknown connection applied")
        } catch let V2Error.refused(code, message) {
            #expect(code == "bad-request")
            #expect(message.hasPrefix("ops[1] connection-secret: "))
            #expect(!message.contains("s3cret"))
        }
        await #expect(throws: V2Error.self) {
            try await c.settingsUpdate(revision: "1", ops: [.workspaceAdd(name: "app", root: "/x")])
        }
        #expect(try await c.settings() == before)
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
