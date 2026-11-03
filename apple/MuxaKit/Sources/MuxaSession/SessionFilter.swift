import MuxaProtocol

/// Which sessions the Sessions list shows (ADR 0025).
public enum SessionFilter: String, CaseIterable, Sendable {
    /// Sessions that run no Task: the user's own tmux work. The default.
    case manual
    /// Every session, Task sessions too, for inspection.
    case all
}

extension Session {
    /// The host associates this session with a Task record. Only `taskId` decides:
    /// what runs or types in its panes never does.
    public var runsTask: Bool { !(taskId ?? "").isEmpty }
}

extension Array where Element == Session {
    /// The sessions `filter` shows, in the same order.
    public func shown(by filter: SessionFilter) -> [Session] {
        switch filter {
        case .manual: self.filter { !$0.runsTask }
        case .all: self
        }
    }
}
