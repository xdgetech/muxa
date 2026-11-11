import Foundation
import MuxaProtocol
import Testing

@testable import MuxaSession

/// A host that opens the overall assistant's root Ask session (`work` level 15, ADR 0054).
final class AskRootBridge: PlanBridge, @unchecked Sendable {
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
                     command: "claude", path: "/assistant", dead: false, shellForeground: false),
            ]))
        }
        emit(.sessionStarted(start: id, session: "$9", name: "ask-\(workspace)"))
        emit(.result(id: id, error: nil, sessions: nil))
    }
}

@MainActor
struct AskRootTests {
    @Test func talkToAssistantOpensTheOverallWorkspacesRootSession() async throws {
        let bridge = AskRootBridge()
        let session = try await PlanTests().connected(bridge, level: "work:15")
        #expect(session.assistantWorkspace == "assistant")
        let started = try await session.talkToAssistant()
        #expect(started == StartedSession(id: "$9", name: "ask-assistant"))
        #expect(session.sessions.contains { $0.id == "$9" })
        // The open session is the answer the second time: one conversation.
        #expect(try await session.talkToAssistant() == started)
        #expect(bridge.roots == ["assistant", "assistant"])
    }

    @Test func aLevel14HostOffersNoTalk() async throws {
        let bridge = AskRootBridge()
        let session = try await PlanTests().connected(bridge, level: "work:14")
        #expect(session.assistantInfo?.overall == "assistant")
        #expect(session.assistantWorkspace == nil)
        let unsupported = PaneSession.Failure.bridge(
            ErrorBody(code: .unsupported, message: PaneSession.assistantUnsupported))
        await #expect(throws: unsupported) { try await session.talkToAssistant() }
        #expect(bridge.roots.isEmpty)
    }
}
