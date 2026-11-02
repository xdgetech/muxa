import Foundation
import MuxaProtocol

// The words the apps use for a Task Loop (ADR 0019). Labels match the Android app.
// A state or reason from a newer bridge keeps its raw name: no meaning is invented
// for it (invariant 6).

/// Shown where the bridge has no `task-loop`: the controls are hidden (ADR 0019).
public let loopNeedsDaemon = "The Task Loop needs the muxa daemon. On the host, run: muxa-bridge daemon install"

extension TaskLoop {
    /// The loop still acts on its own: it can be stopped, not started.
    public var isActive: Bool {
        switch state {
        case .armed, .checking, .paused: true
        case .passed, .stopped, .other: false
        }
    }

    /// A state this app knows the meaning of.
    public var isKnownState: Bool {
        if case .other = state { return false }
        return true
    }

    /// "Armed", "Paused", ...; a state from a newer bridge as it is.
    public var stateTitle: String {
        switch state {
        case .armed: "Armed"
        case .checking: "Checking"
        case .paused: "Paused"
        case .passed: "Passed"
        case .stopped: "Stopped"
        case .other(let raw): raw
        }
    }

    /// One plain sentence: what the loop does now, or why it paused or stopped.
    public var explanation: String {
        switch state {
        case .armed: return "Runs the check after the agent's next turn."
        case .checking: return "The check is running."
        case .passed:
            return lastCheck.map { "The check passed on attempt \($0.attempt)." } ?? "The check passed."
        case .paused:
            if reason == .waiting {
                return "The agent waits for you. The loop resumes after the agent finishes its turn."
            }
            return "Paused: \(reason?.raw ?? "no reason given")."
        case .stopped: return stopExplanation
        case .other(let raw): return "The host reports the state \"\(raw)\"."
        }
    }

    private var stopExplanation: String {
        switch reason {
        case .limit: "Stopped: the check failed \(maxAttempts) times."
        case .timeout: "Stopped: a check ran longer than \(LoopCheck.duration(timeout))."
        case .error: "Stopped: the agent reported an error."
        case .noAgent: "Stopped: no agent runs in this Task's session."
        case .notSent: "Stopped: a prompt could not be typed within 30 seconds."
        case .stalled: "Stopped: the agent did not take the typed prompt."
        case .status: "Stopped: the Task's status is no longer Open."
        case .user: "Stopped by the user."
        case nil: "Stopped."
        case .waiting, .other: "Stopped: \(reason?.raw ?? "")."
        }
    }

    /// "2/3": failed checks so far, of the most allowed.
    public var attemptsLabel: String { "\(attempts)/\(maxAttempts)" }

    /// The Inbox row's badge: "Loop 2/3", "Loop checking", "Loop paused", "Loop passed", "Loop stopped: limit".
    public var badge: String {
        switch state {
        case .armed: return "Loop \(attemptsLabel)"
        case .checking: return "Loop checking"
        case .paused: return "Loop paused"
        case .passed: return "Loop passed"
        case .stopped:
            guard let reason, reason != .user else { return "Loop stopped" }
            return "Loop stopped: \(reason.raw.replacingOccurrences(of: "-", with: " "))"
        case .other(let raw): return "Loop: \(raw)"
        }
    }
}

extension LoopCheck {
    /// "Attempt 2 · exit 1 · 42 s · 5 min ago", or "timed out" in place of the exit code.
    public func line(now: UInt64) -> String {
        let result = exit.map { "exit \($0)" } ?? "timed out"
        let ago = RunLine.duration(now > at ? now - at : 0)
        return "Attempt \(attempt) · \(result) · \(Self.duration(seconds)) · \(ago) ago"
    }

    /// Seconds as "42 s" under a minute, else as a run's duration says it.
    static func duration(_ seconds: UInt64) -> String {
        seconds < 60 ? "\(seconds) s" : RunLine.duration(seconds)
    }
}
