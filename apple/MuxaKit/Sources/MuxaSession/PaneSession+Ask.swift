import Foundation
import MuxaProtocol

/// A session `start-session` started: its id (`$n`) and name.
public struct StartedSession: Equatable, Sendable {
    public var id: String
    public var name: String

    public init(id: String, name: String) {
        self.id = id
        self.name = name
    }
}

/// Ask sessions: the workspace's agent in a repo's main checkout, with no Task (ADR 0031).
extension PaneSession {
    /// Whether the host starts ask sessions, names a branch after the title and takes
    /// relative repos (`work` level 2).
    public var canAsk: Bool { workLevel >= workAsk }

    /// Shown when the host has no ask sessions.
    public static let askUnsupported = "This host cannot start ask sessions. Update muxa on the host."

    /// Whether the host closes ask sessions with `close-task` (`work` level 5, ADR 0039).
    public var canCloseAskSessions: Bool { workLevel >= workCloseAsk && canCloseTasks }

    /// Shown when the host cannot close ask sessions.
    public static let closeAskUnsupported = "This host cannot close ask sessions. Update muxa on the host."

    /// Closes the ask session `s`, then lists again. Its agent stops; the main
    /// checkout and its files stay as they are. Other sessions are never sent.
    public func closeAskSession(_ s: Session) async throws {
        guard canCloseAskSessions, s.ask != nil else {
            throw Failure.bridge(ErrorBody(code: .unsupported, message: Self.closeAskUnsupported))
        }
        try await closeTask(session: s.id, mode: .keep)
    }

    /// Starts an ask session in `repo` and lists sessions again, so the list has it.
    public func startSession(workspace: String, repo: String) async throws -> StartedSession {
        guard canAsk else {
            throw Failure.bridge(ErrorBody(code: .unsupported, message: Self.askUnsupported))
        }
        return try await start { .startSession(id: $0, workspace: workspace, repo: repo) }
    }

    /// The overall assistant's workspace when the host opens its root Ask session:
    /// `work` level 15 and an overall workspace (ADR 0054). Talk to Assistant shows then.
    public var assistantWorkspace: String? {
        workLevel >= workAskRoot ? assistantInfo?.overall : nil
    }

    /// Opens the overall assistant's root Ask session, or starts it, and lists sessions
    /// again (ADR 0054). Its Chat is the user's conversation with the assistant.
    public func talkToAssistant() async throws -> StartedSession {
        guard let workspace = assistantWorkspace else {
            throw Failure.bridge(ErrorBody(code: .unsupported, message: Self.assistantUnsupported))
        }
        return try await start { .startRootSession(id: $0, workspace: workspace) }
    }

    /// Shown when the host has no overall assistant to talk to.
    public static let assistantUnsupported =
        "This host has no overall assistant to talk to. Set one up on the host, or update muxa there."

    /// Sends `make`'s `start-session`, then lists sessions again, so the list has it.
    private func start(_ make: (UInt64) -> Request) async throws -> StartedSession {
        var requestId: UInt64 = 0
        do {
            try await request { id in
                requestId = id
                return make(id)
            }
        } catch {
            startedSessions[requestId] = nil
            throw error
        }
        // The session arrives just before the result.
        guard let started = startedSessions.removeValue(forKey: requestId) else {
            throw Failure.bridge(ErrorBody(code: .internal, message: "The bridge did not name the session."))
        }
        try await refresh()
        return started
    }
}
