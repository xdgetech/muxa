import Foundation
import MuxaProtocol

/// How an item's answers are offered (docs/inbox.md, "The item screen"; ADR 0040):
/// text buttons, More only when two or more would be hidden, and ✓ and Later as
/// icon buttons. Only actions the host listed appear.
public struct ItemAnswers: Equatable, Sendable {
    /// The text buttons, in the host's order.
    public var shown: [InboxAction]
    /// Under More, in the host's order. Never one alone, never ✓ or Later.
    public var more: [InboxAction]
    /// ✓: `keep-open` or `dismiss`, which close only the item.
    public var done: InboxAction?
    /// Later moves the item down.
    public var later: Bool
    /// Command+Return: `reply` while the reply box has text, else the host's first
    /// answer, unless it needs a choice or moves the item down (ADR 0033 §4a.3).
    public var main: InboxAction?
    /// `choose` is offered: the prompt's options are the buttons.
    public var chooses: Bool

    /// `replying`: the reply box has text.
    public init(_ item: InboxItem, replying: Bool = false) {
        let answers = item.answers.filter { $0 != .read }
        chooses = answers.contains(.choose)
        let done = answers.first { $0 == .keepOpen || $0 == .dismiss }
        self.done = done
        later = answers.contains(.later)
        let rest = answers.filter { $0 != .choose && $0 != .later && $0 != done }
        let visible = Self.visible(item.kind)
        let hidden = rest.filter { !visible.contains($0) }
        if hidden.count >= 2 {
            shown = rest.filter { visible.contains($0) }
            more = hidden
        } else {
            shown = rest
            more = []
        }
        if replying, answers.contains(.reply) {
            main = .reply
        } else if replying, answers.contains(.askAgent), !answers.contains(where: { $0.needsText && $0 != .askAgent }) {
            main = .askAgent
        } else {
            main = answers.first.flatMap { $0 == .choose || $0 == .later ? nil : $0 }
        }
    }

    /// The text buttons each kind shows outside More. A `review`'s Redo and Change
    /// Course, and a `failure`'s Change Course and Drop Task, go under More.
    private static func visible(_ kind: ItemKind) -> Set<InboxAction> {
        switch kind {
        case .ask: [.approve, .reject, .reply]
        case .proposal: [.approve, .reject, .askAgent]
        case .review: [.accept, .reply]
        case .failure: [.retry, .reply]
        case .briefing: [.askAgent]
        case .other: [.approve, .reject, .accept, .retry, .reply]
        }
    }
}

extension InboxAction {
    /// Redo, Change course, Send and Ask Agent take the reply box's text, so they need one.
    public var needsText: Bool { self == .redo || self == .changeCourse || self == .reply || self == .askAgent }

    /// The button's title. It names the result (ADR 0033 §4a.2, ADR 0040): on an
    /// `ask` Allow, Deny and Deny & Send; a proposal keeps Approve and Reject; an
    /// answer that changes the Task names it; ✓ is "Done" for VoiceOver.
    public func title(for kind: ItemKind) -> String {
        switch self {
        case .approve: kind == .ask ? "Allow" : "Approve"
        case .reject: kind == .ask ? "Deny" : "Reject"
        case .reply: kind == .ask ? "Deny & Send" : "Send"
        case .choose: "Choose"
        case .accept: "Complete Task"
        case .keepOpen, .dismiss: "Done"
        case .redo: "Redo"
        case .changeCourse: "Change Course"
        case .retry: "Retry"
        case .drop: "Drop Task"
        case .later: "Later"
        case .read: "Mark Read"
        case .askAgent: "Ask Agent"
        }
    }
}

extension InboxItem {
    public var isUnread: Bool { readAt == nil }

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
