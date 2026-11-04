import Foundation
import MuxaProtocol
import MuxaSession

/// The Inbox as a list of questions (ADR 0033), on a host with `work` level 3. An
/// older host has no Inbox; it is told to update muxa-bridge.
extension AppModel {
    var usesItems: Bool { isConnected && session.canUseItems }

    func item(_ id: String) -> InboxItem? { session.inboxItems.first { $0.id == id } }

    /// The session that holds the item's pane, else the one it was made in.
    func itemSession(_ item: InboxItem) -> Session? {
        if let pane = item.pane, let s = session.sessions.first(where: { $0.panes.contains { $0.id == pane } }) {
            return s
        }
        return session.sessions.first { $0.id == item.session && $0.defaultPane != nil }
    }

    /// Changes, docs and images open in the Reader for the item's pane.
    func canOpenEvidence(_ item: InboxItem) -> Bool {
        isConnected && session.canReadDocs && itemSession(item) != nil
    }
}

extension ItemKind {
    /// The item screen's title.
    var title: String {
        switch self {
        case .ask: "Question"
        case .review: "Review"
        case .failure: "Failure"
        case .proposal: "Proposal"
        case .briefing: "Briefing"
        case .other(let raw): raw.capitalized
        }
    }

    var icon: String {
        switch self {
        case .ask: Icon.waiting
        case .review: "checkmark.circle"
        case .failure: Icon.warning
        case .proposal: Icon.proposal
        case .briefing: Icon.briefing
        case .other: "tray"
        }
    }
}
