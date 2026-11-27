#if os(macOS)
import Foundation
import MuxaTransport
import Testing

@testable import MuxaV2

/// Against the real server on the fake backend: `make test-apple-v2` sets
/// MUXA_TEST_V2_SERVER to the built `fake_server` example.
struct ServerTests {
    static let program = ProcessInfo.processInfo.environment["MUXA_TEST_V2_SERVER"]

    /// A fake server on its own socket, killed when the test ends.
    final class Server: @unchecked Sendable {
        let process = Process()
        let socket: String

        init() throws {
            socket = NSTemporaryDirectory() + "muxa-v2-\(UUID().uuidString.prefix(8)).sock"
            process.executableURL = URL(fileURLWithPath: try #require(ServerTests.program))
            process.arguments = [socket]
            process.standardOutput = FileHandle.nullDevice
            try process.run()
            for _ in 0..<100 where !FileManager.default.fileExists(atPath: socket) {
                Thread.sleep(forTimeInterval: 0.05)
            }
        }

        func connect(_ state: V2Client.State = .init()) async throws -> V2Client {
            try await V2Client.open(try UnixSocketBytes(path: socket), name: "test", state: state)
        }

        deinit {
            process.terminate()
            try? FileManager.default.removeItem(atPath: socket)
        }
    }

    /// The next event that is not an Inbox or terminal notice.
    static func next(_ it: inout AsyncStream<V2Event>.Iterator, skipping: (V2Event) -> Bool = { _ in false }) async -> V2Event? {
        while let e = await it.next() {
            if !skipping(e) { return e }
        }
        return nil
    }

    @Test(.enabled(if: program != nil), .timeLimit(.minutes(1)))
    func attachShowsTheScreenAndTypes() async throws {
        let server = try Server()
        let c = try await server.connect()
        let demo = try #require(try await c.terminals().first { $0.name == "demo" })
        var events = c.events.makeAsyncIterator()
        let ch = try await c.attach(demo.id, cols: 80, rows: 24)
        #expect(await Self.next(&events) == .paint(channel: ch))
        #expect(await Self.next(&events) == .output(channel: ch, bytes: Array("hello ".utf8)))
        try await c.input(ch, Array("x".utf8))
        #expect(await Self.next(&events) == .output(channel: ch, bytes: Array("echo:x".utf8)))
        await c.close()
    }

    @Test(.enabled(if: program != nil), .timeLimit(.minutes(1)))
    func theReaderListsAndOpensTheFoldersFiles() async throws {
        let server = try Server()
        let c = try await server.connect()
        let demo = try #require(try await c.terminals().first { $0.name == "demo" })
        let changes = try await c.readerList(demo.id, kind: .changes)
        #expect(changes.root == "demo")
        #expect(changes.entries == [ReaderEntry(path: "a.txt", status: "modified", added: 1, removed: 1)])
        let (_, diff) = try await c.readerOpen(demo.id, kind: .changes, path: "a.txt")
        #expect(diff.text == "@@ -1 +1 @@\n-one\n+two\n")
        let image = try await c.readerOpen(demo.id, kind: .images, path: "shot.png").content
        #expect(image.bytes == Data([0x89, 0x50, 0x4E, 0x47]))
        await #expect(throws: V2Error.self) {
            try await c.readerOpen(demo.id, kind: .docs, path: "../secret.md")
        }
        let zip = try await c.readerFolder(demo.id, folder: "")
        #expect(zip.prefix(4) == Data([0x50, 0x4B, 0x05, 0x06]))
        await #expect(throws: V2Error.self) { try await c.readerFolder(demo.id, folder: "docs") }
        await c.close()
    }

    @Test(.enabled(if: program != nil), .timeLimit(.minutes(1)))
    func aTasksLoopStartsAndStops() async throws {
        let server = try Server()
        let c = try await server.connect()
        let (agent, _) = try await c.startTask(workspace: "w", title: "fix", loop: LoopSpec(check: "make test"))
        let started = try await c.loopStart(agent.id, spec: LoopSpec(), reset: true)
        #expect(started == LoopInfo(terminal: agent.id, check: "make test", maxAttempts: 3, timeout: 600,
                                    state: "armed", attempts: 0, armedAt: 1, since: 1))
        #expect(try await c.loopStop(agent.id).reason == "user")
        await #expect(throws: V2Error.self) {
            try await c.startTask(workspace: "w", title: "bad", loop: LoopSpec(check: "rm -rf /"))
        }
        await c.close()
    }

    @Test(.enabled(if: program != nil), .timeLimit(.minutes(1)))
    func aTasksChatOpensWithItsPrompt() async throws {
        let server = try Server()
        let c = try await server.connect()
        let (agent, _) = try await c.startTask(workspace: "w", title: "chat")
        let (chat, update) = try await c.chatOpen(agent.id)
        #expect(update.entries.map(\.text) == ["Clean the build folder", "Removing it."])
        #expect(update.prompt?.options == [ChatOption(label: "Allow", input: "1"),
                                           ChatOption(label: "Deny", input: "\u{1b}", cancel: true)])
        try await c.chatClose(chat)
        await c.close()
    }

    @Test(.enabled(if: program != nil), .timeLimit(.minutes(1)))
    func diskAsksTheHostAndForceNeedsPaths() async throws {
        let server = try Server()
        let c = try await server.connect()
        #expect(try await c.disk() == [])
        await #expect(throws: V2Error.self) { try await c.diskClean(force: true) }
        await c.close()
    }

    /// The fake host's fixed sample, with a group for each live Task agent (ADR 0105).
    @Test(.enabled(if: program != nil), .timeLimit(.minutes(1)))
    func hostReportsAndWatchSendsAtOnce() async throws {
        let server = try Server()
        let c = try await server.connect()
        let report = try await c.host()
        #expect(report.alerts == ["memory"])
        #expect(report.memory.pressure == .warn)
        #expect(report.systemDisk?.name == "system")
        #expect(report.groups.contains { $0.kind == .simulator && $0.processes == 240 })
        #expect(report.groups.filter { $0.kind == .task }.allSatisfy { $0.terminal != nil })
        var events = c.events.makeAsyncIterator()
        try await c.hostWatch()
        let sent = await Self.next(&events) { if case .host = $0 { false } else { true } }
        guard case .host(let watched)? = sent else { return #expect(Bool(false)) }
        #expect(watched.alerts == ["memory"])
        try await c.hostUnwatch()
        await c.close()
    }

    @Test(.enabled(if: program != nil), .timeLimit(.minutes(1)))
    func workspacesListTheHostsWorkspaces() async throws {
        let server = try Server()
        let c = try await server.connect()
        #expect(try await c.workspaces() == [
            Workspace(name: "Personal", root: "/work", repos: ["app", "docs"], agent: "claude",
                      loop: WorkspaceLoop(checks: .list(["make test"])),
                      connections: [WorkspaceConnection(name: "github", isDefault: true,
                                                        usedBy: UsedBy(routines: ["digest"]), lastUsed: 1_700_000_000)],
                      mcp: [WorkspaceMcp(name: "docs", isDefault: true)],
                      branches: ["app": "main"]),
        ])
        // Every Task agent uses every connection and server of its Workspace (ADR 0075).
        let (agent, _) = try await c.startTask(workspace: "Personal", title: "uses")
        let after = try #require(try await c.workspaces().first)
        #expect(after.connections[0].usedBy.tasks.contains(agent.id))
        #expect(after.mcp[0].usedBy.tasks.contains(agent.id))
        await c.close()
    }

    @Test(.enabled(if: program != nil), .timeLimit(.minutes(1)))
    func aRoutineSaysItWaitsForALimitReset() async throws {
        let server = try Server()
        let c = try await server.connect()
        let routines = try await c.routines().routines
        #expect(routines.map(\.name) == ["digest", "audit"])
        #expect(routines[0].waitingUntil == 1_700_003_600)
        #expect(routines[1].waitingUntil == nil)
        await c.close()
    }

    @Test(.enabled(if: program != nil), .timeLimit(.minutes(1)))
    func subscriptionsComeWithTheirLimits() async throws {
        let server = try Server()
        let c = try await server.connect()
        #expect(try await c.subscriptions() == [SubscriptionUsage(
            name: "claude-work", agent: "claude-code", plan: "Max 20x",
            windows: [
                UsageWindow(minutes: 300, usedPercent: 82, resetsAt: Date(timeIntervalSince1970: 1_764_014_400)),
                UsageWindow(minutes: 10_080, usedPercent: 31, resetsAt: Date(timeIntervalSince1970: 1_764_320_400)),
            ],
            asOf: Date(timeIntervalSince1970: 1_764_000_120))])
        await c.close()
    }

    @Test(.enabled(if: program != nil), .timeLimit(.minutes(1)))
    func tokenUsageComesByWorkspaceAndTask() async throws {
        let server = try Server()
        let c = try await server.connect()
        let usage = try await c.tokenUsage(day: "2026-11-24")
        #expect(usage == TokenUsage(day: "2026-11-24", workspaces: [
            WorkspaceTokens(workspace: "Personal", input: 1_200_000, cached: 9_000_000, output: 84_000),
        ], tasks: [
            TaskTokens(workspace: "Personal", dir: "/work/demo", title: "demo", input: 800_000, cached: 6_000_000, output: 60_000),
        ]))
        await c.close()
    }

    @Test(.enabled(if: program != nil), .timeLimit(.minutes(1)))
    func settingsShowTheHostsFilesWithoutSecrets() async throws {
        let server = try Server()
        let c = try await server.connect()
        #expect(try await c.settings() == Settings(
            revision: "1",
            workspaces: [
                SettingsWorkspace(
                    name: "Personal", root: "~/personal", repos: ["~/src/app"],
                    agent: SettingsAgent(command: "claude", env: [EnvKey(key: "CLAUDE_CONFIG_DIR")]),
                    loop: WorkspaceLoop(checks: .list(["make test"])), routinesEnabled: true,
                    connections: [SettingsConnection(
                        spec: ConnectionSpec(name: "github", env: "GITHUB_TOKEN", account: "me@example.com", access: "read"),
                        secret: .set)],
                    mcp: [McpSpec(name: "docs", command: "npx", args: ["-y", "docs-mcp"])],
                    routines: [SettingsRoutine(
                        spec: RoutineSpec(name: "digest", at: "08:00", timezone: "Europe/Berlin",
                                          instructions: "routines/digest.md"),
                        instructionsText: "Summarize the day.\n")],
                    foundRepos: ["app", "docs"], hiddenRepos: ["docs"]),
            ],
            workers: [WorkerSpec(name: "reviewer", description: "Reviews diffs", agent: "claude-code")],
            subscriptions: [SubscriptionSpec(name: "claude-work", agent: "claude-code", home: "~/.claude-work",
                                             plan: "Max 20x")],
            foundLogins: [FoundLogin(agent: "claude-code", workspace: "Personal")],
            build: SettingsBuild(dir: "/ssd/build", dirFound: true)))
        await c.close()
    }

    @Test(.enabled(if: program != nil), .timeLimit(.minutes(1)))
    func buildUpdateKeepsAbsentFieldsAndClearsNullOnes() async throws {
        let server = try Server()
        let c = try await server.connect()
        let after = try await c.settingsUpdate(revision: "1", ops: [
            .buildUpdate(dropAfter: .set("12h"), extraPaths: ["out", "dist"]),
        ])
        #expect(after.build == SettingsBuild(dir: "/ssd/build", dropAfter: "12h", dirFound: true, extraPaths: ["out", "dist"]))
        let off = try await c.settingsUpdate(revision: after.revision, ops: [.buildUpdate(dir: .clear, extraPaths: [])])
        #expect(off.build == SettingsBuild(dir: nil, dropAfter: "12h", dirFound: false))
        await c.close()
    }

    @Test(.enabled(if: program != nil), .timeLimit(.minutes(1)))
    func settingsUpdateNeverSendsASecretBack() async throws {
        let server = try Server()
        let c = try await server.connect()
        let ops: [SettingsOp] = [
            .envSet(workspace: "Personal", key: "FOO", value: "bar"),
            .connectionAdd(workspace: "Personal", connection: ConnectionSpec(name: "linear", env: "LINEAR_API_KEY"),
                           secret: "s3cret"),
        ]
        let raw = try await c.request(.settingsUpdate(revision: "1", ops: ops))
        let text = String(decoding: raw, as: UTF8.self)
        #expect(!text.contains("s3cret") && !text.contains("bar"))
        let after: Settings = try field("settings", of: raw, decoder: Coding.plainDecoder)
        #expect(after.revision == "2")
        let personal = try #require(after.workspaces.first)
        #expect(personal.agent.env.map(\.key) == ["CLAUDE_CONFIG_DIR", "FOO"])
        #expect(personal.connections.first { $0.name == "linear" }?.secret == .set)
        do {
            _ = try await c.settingsUpdate(revision: "1", ops: [.envUnset(workspace: "Personal", key: "FOO")])
            Issue.record("a stale revision applied")
        } catch let V2Error.refused(code, _) {
            #expect(code == "conflict")
        }
        do {
            _ = try await c.settingsUpdate(revision: "2", ops: [.mcpRemove(workspace: "Personal", name: "nope")])
            Issue.record("an unknown server was removed")
        } catch let V2Error.refused(code, message) {
            #expect(code == "bad-request" && message.hasPrefix("ops[0] mcp-remove: "))
        }
        #expect(try await c.settings() == after)
        await c.close()
    }

    @Test(.enabled(if: program != nil), .timeLimit(.minutes(1)))
    func approveClosesTheTaskAndRequestChangesResolves() async throws {
        let server = try Server()
        let c = try await server.connect()
        let (agent, _) = try await c.startTask(workspace: "w", title: "close")
        let review = try #require(try await c.inbox(.taskReview(terminal: agent.id)).items.first)
        #expect(review.kind == .review && review.terminal == agent.id)
        let approved = try await c.inbox(.inboxApprove(item: review.id))
        #expect(approved.done.contains { $0.id == review.id })
        #expect(!approved.items.contains { $0.id == review.id })
        #expect(try await !c.terminals().contains { $0.id == agent.id })

        let (other, _) = try await c.startTask(workspace: "w", title: "more")
        let second = try #require(try await c.inbox(.taskReview(terminal: other.id)).items.first)
        let changed = try await c.inbox(.inboxRequestChanges(item: second.id, comment: "Add a test"))
        #expect(changed.done.contains { $0.id == second.id })
        #expect(try await c.terminals().contains { $0.id == other.id })
        await c.close()
    }

    @Test(.enabled(if: program != nil), .timeLimit(.minutes(1)))
    func aPausedTaskIsListedAndResumesWithItsItems() async throws {
        let server = try Server()
        let c = try await server.connect()
        try await c.watch()
        var events = c.events.makeAsyncIterator()
        let (agent, _) = try await c.startTask(workspace: "w", title: "wait")
        let review = try #require(try await c.inbox(.taskReview(terminal: agent.id)).items.first)
        let paused = try await c.pauseTask(agent.id)
        #expect(paused.id == agent.id && !paused.terminal.live && paused.terminal.taskTitle == "wait")
        let seen = await Self.next(&events) { if case .paused = $0 { false } else { true } }
        #expect(seen == .paused([paused]))
        #expect(try await c.pausedTasks() == [paused])
        #expect(try await !c.terminals().contains { $0.id == agent.id })

        let (again, _) = try await c.resumeTask(agent.id)
        #expect(again.live && again.taskTitle == "wait" && again.id != agent.id)
        #expect(try await c.pausedTasks().isEmpty)
        #expect(try await c.inbox().items.first { $0.id == review.id }?.terminal == again.id)
        await #expect(throws: V2Error.self) { try await c.resumeTask(agent.id) }
        await c.close()
    }

    @Test(.enabled(if: program != nil), .timeLimit(.minutes(1)))
    func aResumeContinuesWithoutAPaint() async throws {
        let server = try Server()
        let first = try await server.connect()
        let demo = try #require(try await first.terminals().first { $0.name == "demo" })
        var events = first.events.makeAsyncIterator()
        let ch = try await first.attach(demo.id, cols: 80, rows: 24)
        _ = await Self.next(&events)
        _ = await Self.next(&events)
        let state = await first.resumeState
        await first.close()
        try await Task.sleep(for: .milliseconds(200))

        let again = try await server.connect(state)
        #expect(await again.resumed == [ch])
        var more = again.events.makeAsyncIterator()
        try await again.input(ch, Array("y".utf8))
        // No paint and no `hello ` again: only the new output.
        #expect(await Self.next(&more) == .output(channel: ch, bytes: Array("echo:y".utf8)))
        await again.close()
    }

    @Test(.enabled(if: program != nil), .timeLimit(.minutes(1)))
    func aCaptureReachesAWatchingDevice() async throws {
        let server = try Server()
        let phone = try await server.connect()
        try await phone.watch()
        var events = phone.events.makeAsyncIterator()
        let mac = try await server.connect()
        let capture = try await mac.parseCapture("reply to the bank #personal due 12/1", today: "2026-11-25")
        #expect(capture == Capture(kind: .reply, title: "to the bank", workspace: "Personal", due: "2026-12-01"))
        let inbox = try await mac.inbox(.inboxAdd(capture))
        #expect(inbox.items.map(\.title) == ["to the bank"])
        let seen = await Self.next(&events) { if case .inbox = $0 { false } else { true } }
        guard case .inbox(let pushed) = seen else { Issue.record("no inbox event"); return }
        #expect(pushed == inbox)
        let second = try await mac.inbox(.inboxAdd(Capture(kind: .todo, title: "milk")))
        let later = try await mac.inbox(.inboxLater(item: second.items[0].id))
        #expect(later.items.map(\.title) == ["to the bank", "milk"])
        do {
            _ = try await mac.inbox(.inboxDone(item: "nope"))
            Issue.record("done on a missing item")
        } catch let V2Error.refused(code, _) {
            #expect(code == "not-found")
        }
        await phone.close()
        await mac.close()
    }
}
#endif
