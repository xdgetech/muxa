import Foundation
import MuxaProtocol

/// How an item's answers are offered (docs/inbox.md, "The item screen"; ADR 0040).
/// Triage (✓, Later, and the … menu) is the same on every item; the bottom answers
/// the agent: its decisions, then one reply box. Only actions the host listed appear.
public struct ItemAnswers: Equatable, Sendable {
    /// Under the body, in the host's order: Allow, Deny, Approve, Confirm, Reject, Retry.
    public var decisions: [InboxAction]
    /// What the reply box's text does: Send to the item's agent, or Ask Agent where
    /// the item has no session (ADR 0041).
    public var text: InboxAction?
    /// In the top bar's … menu: Complete Task, Drop Task.
    public var menu: [InboxAction]
    /// ✓: `keep-open` or `dismiss`, which close only the item.
    public var done: InboxAction?
    /// Later moves the item down.
    public var later: Bool
    /// Command+Return: the reply box's action while it has text, else the host's
    /// first answer unless it needs a choice, time or text (ADR 0033 §4a.3).
    public var main: InboxAction?
    /// `choose` is offered: the prompt's options are the decisions.
    public var chooses: Bool

    /// `replying`: the reply box has text. `showsWhole`: the screen shows a `write` item's
    /// content (ADR 0052) or a `memory` item's text (ADR 0053) whole and approves it with
    /// its hash; elsewhere neither has Approve.
    public init(_ item: InboxItem, replying: Bool = false, showsWhole: Bool = false) {
        let approves = switch item.kind {
        case .write: showsWhole && item.write != nil
        case .memory: showsWhole && item.memory != nil
        default: true
        }
        let answers = item.answers.filter {
            $0 != .read && $0 != .protect && $0 != .unprotect && (approves || $0 != .approve)
        }
        chooses = answers.contains(.choose)
        done = answers.first { $0 == .keepOpen || $0 == .dismiss }
        later = answers.contains(.later)
        decisions = answers.filter {
            $0 == .approve || $0 == .startTask || $0 == .confirm || $0 == .reject || $0 == .retry
        }
        text = answers.first(where: \.needsText)
        menu = answers.filter { $0 == .accept || $0 == .drop }
        if replying, let text {
            main = text
        } else if answers.contains(.startTask) {
            // Beside a hand-off, Start Task is the main button (ADR 0060 §8).
            main = .startTask
        } else {
            // Never past the first: a question's next answer may be its cancel.
            main = answers.first.flatMap { $0 == .choose || $0 == .later || $0.needsText ? nil : $0 }
        }
    }
}

extension InboxAction {
    /// Send and Ask Agent take the reply box's text, so they need one.
    public var needsText: Bool { self == .reply || self == .askAgent }

    /// The button's title. It names the result (ADR 0033 §4a.2, ADR 0040): on an
    /// `ask` Allow, Deny and Deny & Send; a proposal keeps Approve and Reject; an
    /// answer that changes the Task names it; ✓ is "Done" for VoiceOver. On a `result`
    /// Approve hands off and on a `write` Send revises (ADR 0058 §2, §4). A proposed Task's
    /// answer is Start Task (ADR 0060 §8).
    public func title(for kind: ItemKind) -> String {
        switch self {
        case .approve:
            switch kind {
            case .ask: "Allow"
            case .result: "Hand off"
            case .newTask: "Start Task"
            default: "Approve"
            }
        case .startTask: "Start Task"
        case .reject: kind == .ask ? "Deny" : "Reject"
        case .reply: kind == .ask ? "Deny & Send" : kind == .write ? "Revise" : "Send"
        case .choose: "Choose"
        case .accept: "Complete Task"
        case .keepOpen, .dismiss: "Done"
        case .retry: "Retry"
        case .drop: "Drop Task"
        case .later: "Later"
        case .read: "Mark Read"
        case .askAgent: "Ask Agent"
        case .confirm: "Confirm"
        case .protect: "Protect"
        case .unprotect: "Unprotect"
        }
    }

    /// `title(for:)` with the member it names: "Hand off to writer". On a result that
    /// proposes a Task and no hand-off, Approve is Start Task. On a plan, Approve leaves
    /// plan mode and asks before each edit; Reject and Send keep planning (ADR 0060 §5).
    public func title(for item: InboxItem) -> String {
        if item.isPlan {
            switch self {
            case .approve: return "Approve plan"
            case .reject: return "Keep planning"
            case .reply: return "Keep planning & Send"
            default: break
            }
        }
        if self == .approve, item.kind == .result {
            if let next = item.result?.handoff?.worker { return "Hand off to \(next)" }
            if item.result?.task != nil { return "Start Task" }
        }
        return title(for: item.kind)
    }
}

extension InboxItem {
    public var isUnread: Bool { readAt == nil }
    public var isProtected: Bool { protectedAt != nil }

    /// An `ask` on Claude Code's plan menu (ADR 0060 §5).
    public var isPlan: Bool { kind == .ask && prompt?.kind == .plan }

    /// The Task a `result` or `new-task` item proposes (ADR 0060 §8).
    public var proposedTask: TaskProposal? {
        switch kind {
        case .result: result?.task
        case .newTask: newTask
        default: nil
        }
    }

    /// The answer that starts the proposed Task: `start-task` beside a hand-off, else `approve`.
    public var startsTask: InboxAction? {
        guard proposedTask != nil else { return nil }
        if answers.contains(.startTask) { return .startTask }
        return kind == .newTask || result?.handoff == nil ? answers.first { $0 == .approve } : nil
    }

    /// The staff member that a `result` item's Send or a `write` item's Revise asks again
    /// (ADR 0058 §3, §4). Nil: the workspace's assistant.
    public var askedAgain: String? {
        switch kind {
        case .result: result?.worker
        case .write: write?.worker
        default: nil
        }
    }

    /// Delete Old removes it: made more than `days` days before `now`, and not
    /// protected (ADR 0046). The host's cutoff is whole days before now.
    public func isDeletable(olderThanDays days: UInt32, now: UInt64 = UInt64(Date().timeIntervalSince1970)) -> Bool {
        let cutoff = now > UInt64(days) * 86_400 ? now - UInt64(days) * 86_400 : 0
        return !isProtected && createdAt < cutoff
    }

    /// "45s", "4m", "1h" or "3d" since the item was made.
    public func age(now: UInt64 = UInt64(Date().timeIntervalSince1970)) -> String {
        let s = now > createdAt ? now - createdAt : 0
        if s < 60 { return "\(s)s" }
        if s < 3600 { return "\(s / 60)m" }
        if s < 86_400 { return "\(s / 3600)h" }
        return "\(s / 86_400)d"
    }

    /// The row's second line: the Task title or the Routine, then the age.
    public func source(now: UInt64 = UInt64(Date().timeIntervalSince1970)) -> String {
        "\(routine ?? title) · \(age(now: now))"
    }
}
