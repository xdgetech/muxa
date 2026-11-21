import Foundation
import MuxaProtocol
import Testing

@testable import MuxaSession

/// A host that opens a workspace's root Ask session (`work` level 15, ADR 0054).
final class AskRootBridge: FakeBridge, @unchecked Sendable {
    private let lock = NSLock()
    private var _roots: [String] = []
    /// The workspaces `start-session` with `root` was sent for.
    var roots: [String] { lock.withLock { _roots } }

    override func send(_ line: Data) async throws {
        guard case .startRootSession(let id, let workspace) = try Wire.decodeRequest(line) else {
            return try await super.send(line)
        }
        lock.withLock { _roots.append(workspace) }
        if !sessions.contains(where: { $0.id == "$9" }) {
            sessions.append(Session(id: "$9", name: "ask-\(workspace)", ask: workspace, panes: [
                Pane(id: "%9", window: "@9", index: 0, role: .agent, cols: 20, rows: 5,
                     command: "claude", path: "/work", dead: false, shellForeground: false),
            ]))
        }
        emit(.sessionStarted(start: id, session: "$9", name: "ask-\(workspace)"))
        emit(.result(id: id, error: nil, sessions: nil))
    }
}

@MainActor
struct AskRootTests {
    func connected(_ bridge: AskRootBridge, level: String) async throws -> PaneSession {
        bridge.capabilities = ["task-records", level]
        let session = PaneSession()
        try await session.connect {
            bridge.start()
            return bridge
        }
        return session
    }

    @Test func theRootSessionIsOpenedOrStarted() async throws {
        let bridge = AskRootBridge()
        let session = try await connected(bridge, level: "work:15")
        #expect(session.canStartRootSession)
        let started = try await session.startRootSession(workspace: "work")
        #expect(started == StartedSession(id: "$9", name: "ask-work"))
        #expect(session.sessions.contains { $0.id == "$9" })
        // The open session is the answer the second time: one conversation.
        #expect(try await session.startRootSession(workspace: "work") == started)
        #expect(bridge.roots == ["work", "work"])
    }

    @Test func aLevel14HostIsNeverAsked() async throws {
        let bridge = AskRootBridge()
        let session = try await connected(bridge, level: "work:14")
        #expect(!session.canStartRootSession)
        let unsupported = PaneSession.Failure.bridge(ErrorBody(code: .unsupported, message: PaneSession.askUnsupported))
        await #expect(throws: unsupported) { try await session.startRootSession(workspace: "work") }
        #expect(bridge.roots.isEmpty)
    }
}
