import MuxaProtocol

/// One row of a task's Runs (ADR 0018). The Android client shows the same text.
public struct RunLine: Equatable, Sendable, Identifiable {
    public var id: UInt32
    /// "Run 2 · claude-code".
    public var title: String
    /// "Restarted · 1 h ago · 42 min".
    public var detail: String
    public var agentSession: String?
    public var assignment: String? = nil
}

extension InboxItem {
    /// The task's runs, newest first. A run without `endedAt` is said to run only
    /// while an adapter reports the task's agent (invariant 6).
    public func runLines(now: UInt64) -> [RunLine] {
        let newest = record.runs.max { $0.id < $1.id }?.id
        return record.runs.sorted { $0.id > $1.id }.map { run in
            let tail: String
            if let outcome = run.outcome {
                tail = outcome == "unconfirmed" ? "unconfirmed outcome" : "unknown outcome"
            } else if let ended = run.endedAt {
                tail = RunLine.duration(ended.saturatingMinus(run.startedAt))
            } else if run.id == newest, agent != nil {
                tail = "still running"
            } else {
                tail = "end not recorded"
            }
            let started = RunLine.duration(now.saturatingMinus(run.startedAt)) + " ago"
            return RunLine(
                id: run.id, title: "Run \(run.id) · \(run.agent ?? "unknown agent")",
                detail: [RunLine.reason(run.reason), started, tail].joined(separator: " · "),
                agentSession: run.agentSession, assignment: run.assignment?.displayName ?? "Worker unrecorded")
        }
    }

    /// A replacement with no fresh state may still be working. Stopping it
    /// requires the same explicit confirmation as an observed running agent.
    public var restartNeedsForce: Bool {
        agent?.state == .running
            || (agent == nil && record.runs.max { $0.id < $1.id }?.reason == .restart)
    }

    /// The bridge writes `.muxa/HANDOFF.md` only when it restarts the agent.
    public var hasHandoff: Bool { record.runs.contains { $0.reason == .restart } }
}

extension RunLine {
    static func reason(_ reason: RunReason) -> String {
        switch reason {
        case .start: "Started"
        case .restart: "Restarted"
        case .other(let raw): raw
        }
    }

    /// "<1 min", "42 min", "1 h", "1 h 5 min". Hours are not split into days.
    static func duration(_ seconds: UInt64) -> String {
        if seconds < 60 { return "<1 min" }
        if seconds < 3600 { return "\(seconds / 60) min" }
        let minutes = seconds % 3600 / 60
        return minutes == 0 ? "\(seconds / 3600) h" : "\(seconds / 3600) h \(minutes) min"
    }
}

private extension UInt64 {
    /// A clock that went backwards reads as no time, not a crash.
    func saturatingMinus(_ other: UInt64) -> UInt64 { self > other ? self - other : 0 }
}
