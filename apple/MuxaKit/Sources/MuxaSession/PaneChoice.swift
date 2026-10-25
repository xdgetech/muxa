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

extension Agent {
    /// The state to show. A shell in the foreground means the runner's state is likely stale.
    public func shown(shellForeground: Bool) -> AgentState? {
        shellForeground ? nil : state
    }
}
