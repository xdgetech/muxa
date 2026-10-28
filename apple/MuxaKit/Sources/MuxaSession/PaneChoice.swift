import MuxaProtocol

/// Which pane the Agent/Terminal toggle shows (PROTOCOL.md, "Agent/Terminal toggle").
extension Session {
    var visiblePanes: [Pane] { panes.filter { $0.role != .hidden } }

    /// The pane to open first: the agent, else the first visible pane.
    public var defaultPane: Pane? {
        visiblePanes.first { $0.role == .agent } ?? visiblePanes.first
    }

    /// The other side of the toggle for `paneId`, or nil when the toggle is disabled.
    public func counterpart(of paneId: String) -> Pane? {
        guard let pane = panes.first(where: { $0.id == paneId }) else { return nil }
        switch pane.role {
        case .agent: return visiblePanes.first { $0.role == .shell }
        case .shell: return visiblePanes.first { $0.role == .agent }
        case .hidden: return nil
        case .unknown:
            // No tags: the shell in the same window is the terminal.
            if pane.shellForeground {
                return visiblePanes.first { $0.window == pane.window && $0.id != pane.id && !$0.shellForeground }
            }
            return visiblePanes.first { $0.window == pane.window && $0.id != pane.id && $0.shellForeground }
        }
    }
}

/// The pane last shown on a host. The session is kept by name: tmux renumbers
/// its ids when the server restarts, and a pane id alone may then be another's.
public struct LastPane: Codable, Equatable, Sendable {
    public var session: String
    public var pane: String

    public init(session: String, pane: String) {
        self.session = session
        self.pane = pane
    }
}

extension Array where Element == Session {
    /// The pane to reopen for `last`: that pane while it is still in that
    /// session, else the session's default pane. Nil once the session is gone.
    public func resuming(_ last: LastPane) -> Pane? {
        guard let session = first(where: { $0.name == last.session }) else { return nil }
        return session.visiblePanes.first { $0.id == last.pane } ?? session.defaultPane
    }
}

extension Agent {
    /// The state to show. A shell in the foreground means the runner's state is likely stale.
    public func shown(shellForeground: Bool) -> AgentState? {
        shellForeground ? nil : state
    }
}

/// Sessions of one repo, as the agent runner tags them (`@agent_repo`).
public struct RepoGroup: Equatable, Sendable {
    /// Nil for sessions without a repo tag.
    public var repo: String?
    public var sessions: [Session]
}

extension Array where Element == Session {
    /// Groups by repo, in the order each repo first appears in tmux's list, with
    /// untagged sessions last. Within a group, tmux's order is kept.
    public func groupedByRepo() -> [RepoGroup] {
        var groups: [RepoGroup] = []
        var untagged: [Session] = []
        for session in self {
            guard let repo = session.repo, !repo.isEmpty else {
                untagged.append(session)
                continue
            }
            if let i = groups.firstIndex(where: { $0.repo == repo }) {
                groups[i].sessions.append(session)
            } else {
                groups.append(RepoGroup(repo: repo, sessions: [session]))
            }
        }
        if !untagged.isEmpty { groups.append(RepoGroup(repo: nil, sessions: untagged)) }
        return groups
    }
}
