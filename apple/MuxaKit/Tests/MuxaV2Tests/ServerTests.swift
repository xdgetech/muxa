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
    func workspacesListTheHostsWorkspaces() async throws {
        let server = try Server()
        let c = try await server.connect()
        #expect(try await c.workspaces() == [
            Workspace(name: "Personal", root: "/work", repos: ["app", "docs"], agent: "claude",
                      loop: WorkspaceLoop(checks: .list(["make test"])),
                      connections: [WorkspaceConnection(name: "github", isDefault: true,
                                                        usedBy: UsedBy(routines: ["digest"]), lastUsed: 1_700_000_000)],
                      mcp: [WorkspaceMcp(name: "docs", isDefault: false)]),
        ])
        // github is on by default: the new Task's agent uses it.
        let (agent, _) = try await c.startTask(workspace: "Personal", title: "uses", mcp: ["docs"])
        let after = try #require(try await c.workspaces().first)
        #expect(after.connections[0].usedBy.tasks.contains(agent.id))
        #expect(after.mcp[0].usedBy.tasks == [agent.id])
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
