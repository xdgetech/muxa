import MuxaV2
import SwiftUI

// How a Bot and its work read, on every platform (ADR 0086).

extension BotWork.Status {
    var title: String {
        switch self {
        case .needsYou: "Needs you"
        case .running: "Running"
        case .paused: "Paused"
        case .idle: "Idle"
        }
    }

    /// The list's dot: amber when it needs the user, green while it runs.
    var dot: Color? {
        switch self {
        case .needsYou: TaskState.needsYou.color
        case .running: Palette.okText
        case .paused, .idle: nil
        }
    }

    var badgeForeground: Color {
        switch self {
        case .needsYou: TaskState.needsYou.color
        case .running: Palette.okText
        case .paused, .idle: Palette.textSecondary
        }
    }

    var badgeBackground: Color {
        switch self {
        case .needsYou: Palette.bannerBack
        case .running: Palette.okBack
        case .paused, .idle: Palette.pill
        }
    }
}

extension BotEntry {
    /// `2 need you · next scan 16:30`, `Running · 1 run`, `Paused`, `Idle · On review`.
    var statusLine: String {
        var parts: [String]
        switch work.status {
        case .needsYou: parts = ["\(work.needsYou.count) need\(work.needsYou.count == 1 ? "s" : "") you"]
        case .running: parts = [work.terminals.count == 1 ? "Running" : "Running · \(work.terminals.count)"]
        case .paused: return "Paused"
        case .idle: parts = ["Idle"]
        }
        if let next = work.planned.first { parts.append(next.plannedText) }
        return parts.joined(separator: " · ")
    }
}

extension Routine {
    /// `next scan Nov 23, 16:30`, or `scan on review`.
    var plannedText: String {
        if let due = nextDue { return "next \(name) \(routineStamp(due))" }
        return "\(name) on review"
    }
}

extension MuxaV2.Terminal {
    /// What the terminal is to the Bot: `Chat`, `Run of scan`, `research, a member`, with its state.
    func botRole(of bot: String) -> String {
        let role: String
        if lead == bot {
            role = "\(self.bot ?? "a member"), a member"
        } else if isTaskAgent {
            role = "Chat"
        } else if let r = routineName {
            role = "Run of \(r)"
        } else {
            role = labels["muxa.kind"] == "handoff" ? "Run as a member" : "Run"
        }
        guard let state = status?.state else { return role }
        return "\(role) · \(TaskState(state).title.lowercased())"
    }
}

/// A Unix time as the Routines page prints it.
func routineStamp(_ seconds: Int64) -> String {
    Date(timeIntervalSince1970: TimeInterval(seconds)).formatted(date: .abbreviated, time: .shortened)
}
