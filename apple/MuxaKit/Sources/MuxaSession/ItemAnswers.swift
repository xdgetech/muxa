import Foundation
import MuxaProtocol

/// How an item's answers are offered (docs/inbox.md, "The item screen"): two or
/// three buttons, and the rest under More. Only actions the host listed appear.
public struct ItemAnswers: Equatable, Sendable {
    /// The buttons, in the host's order.
    public var shown: [InboxAction]
    /// Under More, in the host's order. Later is last; Open Terminal is the app's.
    public var more: [InboxAction]
    /// Command+Return: the host's first answer, unless it needs a choice or a time.
    public var main: InboxAction?
    /// `choose` is offered: the prompt's options are the buttons.
    public var chooses: Bool

    public init(_ item: InboxItem) {
        let answers = item.answers.filter { $0 != .read }
        let visible = Self.visible(item.kind)
        chooses = answers.contains(.choose)
        shown = answers.filter { visible.contains($0) }
        more = answers.filter { !visible.contains($0) && $0 != .choose && $0 != .later }
            + (answers.contains(.later) ? [.later] : [])
        main = answers.first.flatMap { $0 == .choose || $0 == .later ? nil : $0 }
    }

    /// The buttons each kind shows outside More.
    private static func visible(_ kind: ItemKind) -> Set<InboxAction> {
        switch kind {
        case .ask, .proposal: [.approve, .reject]
        case .review: [.accept, .redo]
        case .failure: [.retry, .changeCourse]
        case .briefing: [.dismiss]
        case .other: [.approve, .reject, .accept, .retry, .dismiss]
        }
    }
}

extension InboxAction {
    /// Redo and Change course send the reply to the agent, so they need one.
    public var needsText: Bool { self == .redo || self == .changeCourse }

    /// The button's title. A briefing's dismissal reads as Done.
    public func title(for kind: ItemKind) -> String {
        switch self {
        case .approve: "Approve"
        case .reject: "Reject"
        case .choose: "Choose"
        case .accept: "Accept"
        case .keepOpen: "Keep Open"
        case .redo: "Redo"
        case .changeCourse: "Change Course"
        case .retry: "Retry"
        case .drop: "Drop"
        case .dismiss: kind == .briefing ? "Done" : "Dismiss"
        case .later: "Later"
        case .read: "Mark Read"
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
