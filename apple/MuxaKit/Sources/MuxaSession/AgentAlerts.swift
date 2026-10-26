import MuxaProtocol

/// An agent elsewhere that wants attention.
public struct AgentAlert: Equatable, Sendable, Identifiable {
    public enum Kind: Sendable {
        case waiting, done, failed
    }

    public var session: String
    public var sessionName: String
    public var pane: String
    public var kind: Kind

    public var id: String { "\(pane):\(kind)" }

    public var message: String {
        switch kind {
        case .waiting: "needs your input"
        case .done: "finished"
        case .failed: "failed"
        }
    }

    public init(session: String, sessionName: String, pane: String, kind: Kind) {
        self.session = session
        self.sessionName = sessionName
        self.pane = pane
        self.kind = kind
    }
}

extension Array where Element == Session {
    /// What changed for the agents between `old` and this list, worth telling:
    /// an agent now waiting for input, finished, or failed. The shown session is
    /// left out, and so are panes `old` did not have, e.g. on the first list.
    public func alerts(since old: [Session], shown: String?) -> [AgentAlert] {
        var before: [String: AgentState] = [:]
        var known: Set<String> = []
        for pane in old.flatMap(\.panes) {
            known.insert(pane.id)
            if let state = Self.state(pane) { before[pane.id] = state }
        }
        return flatMap { s -> [AgentAlert] in
            guard s.id != shown else { return [] }
            return s.panes.compactMap { pane in
                guard known.contains(pane.id), let now = Self.state(pane), now != before[pane.id],
                    let kind = Self.kind(from: before[pane.id], to: now)
                else { return nil }
                return AgentAlert(session: s.id, sessionName: s.name, pane: pane.id, kind: kind)
            }
        }
    }

    /// A state is not believed while a shell is in front: the agent likely exited.
    private static func state(_ pane: Pane) -> AgentState? {
        pane.agent?.shown(shellForeground: pane.shellForeground)
    }

    /// Running to idle is how an agent without a "done" state ends its turn.
    private static func kind(from old: AgentState?, to new: AgentState) -> AgentAlert.Kind? {
        switch new {
        case .waiting: .waiting
        case .done: .done
        case .error: .failed
        case .idle where old == .running: .done
        case .idle, .running: nil
        }
    }
}
