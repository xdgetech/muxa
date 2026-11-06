import Foundation
import MuxaProtocol

/// A Task's end removes its worktree (ADR 0043, `work` level 7).
extension PaneSession {
    public var canEndWorktrees: Bool { workLevel >= workWorktreeEnd }

    /// What to send with Done or Drop: nothing to an older host, which keeps the worktree.
    func worktreeEnd(_ wanted: WorktreeEnd) -> WorktreeEnd? {
        canEndWorktrees ? wanted : nil
    }

    /// The host's reason when an end with `.remove` would lose work, e.g. "its branch
    /// is not merged". Nil for any other error.
    public nonisolated static func lostWork(_ error: any Error) -> String? {
        guard case Failure.bridge(let body) = error, body.code == .wouldLoseWork else { return nil }
        return body.message
    }

    /// The confirmation's text for `reason`.
    public nonisolated static func removeAnyway(_ reason: String) -> String {
        "\(reason.prefix(1).uppercased())\(reason.dropFirst()). Remove its worktree and branch anyway?"
    }
}
