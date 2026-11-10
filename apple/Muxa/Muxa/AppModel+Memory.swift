import Foundation
import MuxaProtocol
import MuxaSession

/// Workspace memory (ADR 0053), on a host with `work` level 14. Only the item screen
/// approves a proposal, after it showed the text whole.
extension AppModel {
    var usesMemory: Bool { usesItems && session.canUseMemory }

    /// The item screen shows this proposal whole and may offer Approve.
    func showsMemory(_ item: InboxItem) -> Bool { usesMemory && item.kind == .memory && item.memory != nil }

    /// The item screen shows a write's content or a memory proposal's text whole.
    func showsWhole(_ item: InboxItem) -> Bool { showsWrite(item) || showsMemory(item) }

    /// The Task detail's line after the end: "Added to the workspace memory", or
    /// "Proposed to the workspace memory" with the item's state.
    func shown(_ status: MemoryStatus) -> String {
        switch status {
        case .added: "Added to the workspace memory"
        case .waiting: "Proposed to the workspace memory · Waiting in the Inbox"
        case .rejected: "Proposed to the workspace memory · Rejected"
        case .closed: "Proposed to the workspace memory · Closed without an answer"
        }
    }

    /// "Review · changed 3h ago": the workspace's memory mode and its file's last change.
    /// Nil below level 14.
    func memoryShown(of workspace: String?) -> String? {
        guard let mode = session.memoryMode(of: workspace) else { return nil }
        let name = switch mode {
        case .review: "Review"
        case .auto: "Auto"
        case .off: "Off"
        case .other(let raw): "Unknown (\(raw))"
        }
        guard mode != .off else { return name }
        let changed = session.memoryChangedAt(of: workspace).map { "changed \(AgentBadge.age($0)) ago" } ?? "no file yet"
        return "\(name) · \(changed)"
    }
}
