import MuxaProtocol

extension Session {
    /// The host associates this session with a Task record. Only `taskId` decides:
    /// what runs or types in its panes never does.
    public var runsTask: Bool { !(taskId ?? "").isEmpty }
}

extension Array where Element == Session {
    /// The sessions that run no Task, in the same order. The Sessions list shows
    /// only these (ADR 0027); Task sessions open from Inbox and Tasks.
    public var manual: [Session] { filter { !$0.runsTask } }
}
