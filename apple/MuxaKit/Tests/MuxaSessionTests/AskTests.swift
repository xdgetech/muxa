import Foundation
import MuxaProtocol
import Testing

@testable import MuxaSession

/// A bridge that starts ask sessions as the host does (ADR 0031).
final class AskBridge: FakeBridge, @unchecked Sendable {
    /// False: the session starts but the bridge never names it.
    var namesSession = true

    override func send(_ line: Data) async throws {
        let req = try Wire.decodeRequest(line)
        if case .closeTask(let id, let target, _) = req {
            lock.withLock { _closes.append(target) }
            guard sessions.contains(where: { $0.id == target && $0.ask != nil }) else {
                emit(.result(id: id, error: ErrorBody(code: .badRequest, message: "\(target) is not an ask session"), sessions: nil))
                return
            }
            sessions.removeAll { $0.id == target }
            emit(.result(id: id, error: nil, sessions: nil))
            return
        }
        guard case .startSession(let id, let workspace, let repo) = req else { return try await super.send(line) }
        lock.withLock { _asks.append(req) }
        guard workspace == "work", repo == "/src/app" else {
            emit(.result(id: id, error: ErrorBody(code: .badRequest, message: "\(repo) is not a repo of \(workspace)"), sessions: nil))
            return
        }
        sessions.append(Session(id: "$7", name: "ask-app-1", repo: "app", repoDir: repo, ask: workspace, panes: [
            Pane(id: "%7", window: "@7", index: 0, role: .agent, cols: 20, rows: 5,
                 command: "claude", path: repo, dead: false, shellForeground: false),
        ]))
        if namesSession { emit(.sessionStarted(start: id, session: "$7", name: "ask-app-1")) }
        emit(.result(id: id, error: nil, sessions: nil))
    }

    private let lock = NSLock()
    private var _asks: [Request] = []
    var asks: [Request] { lock.withLock { _asks } }
    private var _closes: [String] = []
    /// The session ids `close-task` was sent for.
    var closes: [String] { lock.withLock { _closes } }
}

@MainActor
struct AskTests {
    func connected(_ bridge: AskBridge) async throws -> PaneSession {
        let session = PaneSession()
        try await session.connect {
            bridge.start()
            return bridge
        }
        return session
    }

    @Test func anAskSessionStartsAndIsListed() async throws {
        let bridge = AskBridge()
        bridge.capabilities = ["task-records", "work:2"]
        let session = try await connected(bridge)
        #expect(session.canAsk)
        let started = try await session.startSession(workspace: "work", repo: "/src/app")
        #expect(started == StartedSession(id: "$7", name: "ask-app-1"))
        let listed = try #require(session.sessions.first { $0.id == "$7" })
        #expect(listed.taskId == nil, "a manual session")
        #expect(bridge.asks == [.startSession(id: bridge.asks.first?.id ?? 0, workspace: "work", repo: "/src/app")])
    }

    @Test func theHostsRefusalIsShown() async throws {
        let bridge = AskBridge()
        bridge.capabilities = ["work:2"]
        let session = try await connected(bridge)
        await #expect(throws: PaneSession.Failure.bridge(ErrorBody(code: .badRequest, message: "/elsewhere is not a repo of work"))) {
            try await session.startSession(workspace: "work", repo: "/elsewhere")
        }
    }

    @Test func aMissingSessionStartedIsAnError() async throws {
        let bridge = AskBridge()
        bridge.capabilities = ["work:2"]
        bridge.namesSession = false
        let session = try await connected(bridge)
        await #expect(throws: PaneSession.Failure.bridge(ErrorBody(code: .internal, message: "The bridge did not name the session."))) {
            try await session.startSession(workspace: "work", repo: "/src/app")
        }
    }

    @Test func anAskSessionCloses() async throws {
        let bridge = AskBridge()
        bridge.capabilities = ["close-task", "task-records", "work:5"]
        let session = try await connected(bridge)
        #expect(session.canCloseAskSessions)
        _ = try await session.startSession(workspace: "work", repo: "/src/app")
        let ask = try #require(session.sessions.first { $0.id == "$7" })
        #expect(ask.ask == "work")
        try await session.closeAskSession(ask)
        #expect(bridge.closes == ["$7"])
        #expect(!session.sessions.contains { $0.id == "$7" }, "listed again")
    }

    @Test func aSessionWithoutAskOrAnOlderHostIsNeverClosed() async throws {
        let bridge = AskBridge()
        bridge.capabilities = ["close-task", "task-records", "work:4"]
        let session = try await connected(bridge)
        #expect(!session.canCloseAskSessions)
        _ = try await session.startSession(workspace: "work", repo: "/src/app")
        let ask = try #require(session.sessions.first { $0.id == "$7" })
        let unsupported = PaneSession.Failure.bridge(ErrorBody(code: .unsupported, message: PaneSession.closeAskUnsupported))
        await #expect(throws: unsupported) { try await session.closeAskSession(ask) }
        let newerBridge = AskBridge()
        newerBridge.capabilities = ["close-task", "work:5"]
        let newer = try await connected(newerBridge)
        let manual = try #require(newer.sessions.first { $0.id == "$1" })
        await #expect(throws: unsupported) { try await newer.closeAskSession(manual) }
        #expect(bridge.closes.isEmpty && newerBridge.closes.isEmpty)
    }

    @Test func anOlderHostIsNeverAsked() async throws {
        let bridge = AskBridge()
        bridge.capabilities = ["task-records", "work:1"]
        let session = try await connected(bridge)
        #expect(!session.canAsk)
        await #expect(throws: PaneSession.Failure.bridge(ErrorBody(code: .unsupported, message: PaneSession.askUnsupported))) {
            try await session.startSession(workspace: "work", repo: "/src/app")
        }
        #expect(bridge.asks.isEmpty)
    }
}

struct RepoChoiceTests {
    @Test func aRepoUnderTheRootShowsItsRelativePath() {
        #expect(RepoChoice.displayName("/Users/me/lisk/lisk-app", root: "/Users/me/lisk") == "lisk-app")
        #expect(RepoChoice.displayName("/Users/me/fp/fairpull/fairpull-plan", root: "/Users/me/fp/") == "fairpull/fairpull-plan")
    }

    @Test func aRepoElsewhereShowsTheHomeAsATilde() {
        #expect(RepoChoice.displayName("/Users/me/src/tmux", root: "/Users/me/lisk") == "~/src/tmux")
        #expect(RepoChoice.displayName("/home/dev/forks/tmux", root: nil) == "~/forks/tmux")
        #expect(RepoChoice.displayName("/root/app", root: nil) == "~/app")
        #expect(RepoChoice.displayName("/opt/app", root: "/src") == "/opt/app")
        // A sibling folder that shares the root's prefix is not under it.
        #expect(RepoChoice.displayName("/src-old/app", root: "/src") == "/src-old/app")
    }

    @Test func theChoicesAreTheWorkspacesRepos() {
        let workspaces = [
            WorkspaceInfo(name: "work", root: "/src", repos: ["/home/dev/tmux", "/src/app", "/src/libs/core"]),
            WorkspaceInfo(name: "notes", root: "/notes"),
        ]
        let known = [KnownRepo(name: "other", dir: "/other")]
        #expect(workspaces.repoChoices(in: "work", known: known).map(\.name) == ["~/tmux", "app", "libs/core"])
        #expect(workspaces.repoChoices(in: "work", known: known).map(\.dir) == ["/home/dev/tmux", "/src/app", "/src/libs/core"])
        #expect(workspaces.repoChoices(in: "notes", known: known).isEmpty, "another workspace's repos are not offered")
    }

    @Test func anOlderHostOffersTheReposItHasSessionsIn() {
        let workspaces = [WorkspaceInfo(name: "work"), WorkspaceInfo(name: "notes")]
        let known = [KnownRepo(name: "app", dir: "/src/app")]
        #expect(workspaces.repoChoices(in: "notes", known: known) == [RepoChoice(dir: "/src/app", name: "app")])
    }

    @Test func aRepoBelongsToTheWorkspaceThatListsItOrHoldsIt() {
        let workspaces = [
            WorkspaceInfo(name: "a", root: "/a", repos: ["/shared/x"]),
            WorkspaceInfo(name: "b", root: "/b"),
        ]
        #expect(workspaces.workspace(of: "/shared/x")?.name == "a")
        #expect(workspaces.workspace(of: "/b/y")?.name == "b")
        #expect(workspaces.workspace(of: "/c/z") == nil)
    }
}
