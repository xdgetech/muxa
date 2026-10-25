import MuxaProtocol

/// The conversation of the agent in one pane, as `chat-entries` build it up.
public struct ChatLog: Equatable, Sendable {
    public let pane: String
    public private(set) var entries: [ChatEntry] = []
    /// The agent's state as of the last update; nil when unknown.
    public private(set) var status: Agent?
    /// The adapter, e.g. `claude-code`.
    public private(set) var agent: String?
    /// Older history exists but was not sent.
    public private(set) var truncated = false
    /// Why the bridge stopped following, if it did.
    public internal(set) var ended: String?
    /// The first update has arrived.
    public private(set) var loaded = false
    private var seq: UInt64 = 0
    private var index: [String: Int] = [:]

    public init(pane: String) {
        self.pane = pane
    }

    /// Applies one update. An update at or below the last `seq` was already
    /// covered, for example by a resend, and is dropped.
    public mutating func apply(_ update: ChatUpdate) {
        guard update.seq > seq else { return }
        seq = update.seq
        if update.reset {
            entries = []
            index = [:]
            agent = update.agent
            truncated = update.truncated
        }
        for entry in update.entries {
            if let i = index[entry.id] {
                entries[i] = entry
            } else {
                index[entry.id] = entries.count
                entries.append(entry)
            }
        }
        status = update.status
        ended = nil
        loaded = true
    }
}
