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
        var requestId: UInt64 = 0
        do {
            try await request { id in
                requestId = id
                return .startSession(id: id, workspace: workspace, repo: repo)
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
