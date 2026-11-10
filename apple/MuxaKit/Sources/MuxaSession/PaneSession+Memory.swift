import CryptoKit
import Foundation
import MuxaProtocol

/// Workspace memory (ADR 0053) on a host with `work` level 14: a Task's end proposes text
/// for `.muxa/WORKSPACE.md`. Approve carries the hash of the text the screen showed whole;
/// nothing else approves one.
extension PaneSession {
    public var canUseMemory: Bool { workLevel >= workMemory }

    /// Shown when the host keeps no workspace memory.
    public static let memoryUnsupported = "This host keeps no workspace memory. Update muxa on the host."
    /// Shown when approve does not come from the screen that shows the whole text, or the
    /// shown text is not the listed one.
    public static let memoryNotShown = "Open the proposal and read its whole text before you approve it."

    /// What a Task's end does in `workspace`. Nil below level 14 and for a workspace the
    /// host does not list.
    public func memoryMode(of workspace: String?) -> MemoryMode? {
        guard canUseMemory else { return nil }
        return workspaces.first { $0.name == workspace }?.memory
    }

    /// Epoch seconds of the last change to the workspace's `WORKSPACE.md`; nil when it does
    /// not exist, and below level 14.
    public func memoryChangedAt(of workspace: String?) -> UInt64? {
        guard canUseMemory else { return nil }
        return workspaces.first { $0.name == workspace }?.memoryAt
    }

    /// Approves a `memory` item. `shown` is the proposal as the screen showed it whole; its
    /// hash, computed again here from what was shown, is what the host checks. A proposal
    /// that differs from the listed item, or whose text does not hash to its `hash`, is
    /// refused here and nothing is sent.
    @discardableResult
    public func approveMemory(_ item: InboxItem, shown: MemoryProposal) async throws -> InboxItem {
        guard canUseMemory else {
            throw Failure.bridge(ErrorBody(code: .unsupported, message: Self.memoryUnsupported))
        }
        let listed = inboxItems.first { $0.id == item.id }?.memory ?? item.memory
        let hash = MemoryHash.of(task: shown.task, title: shown.title, text: shown.text)
        guard item.kind == .memory, listed == shown, hash == shown.hash else {
            throw Failure.bridge(ErrorBody(code: .conflict, message: Self.memoryNotShown))
        }
        return try await respond(item, .approve, text: nil, option: nil, worktree: .remove, hash: hash)
    }

    /// The `memory` item a Task's end made. A Task has at most one.
    public func memoryItem(of task: String) -> InboxItem? {
        guard canUseMemory else { return nil }
        return inboxItems.first { $0.kind == .memory && ($0.memory?.task ?? $0.task) == task }
    }

    /// What became of the Task's proposal to the workspace memory, as its item records it.
    /// Nil when the Task proposed nothing, or the host appended it at once (`auto`).
    public func memoryStatus(of task: String) -> MemoryStatus? {
        guard let item = memoryItem(of: task) else { return nil }
        switch item.state {
        case .open, .later: return .waiting(item: item.id)
        case .answered where item.answer == .approve: return .added(item: item.id)
        case .answered where item.answer == .reject: return .rejected(item: item.id)
        case .answered, .resolved, .other: return .closed(item: item.id)
        }
    }
}

/// A Task's proposal to the workspace memory, in its detail.
public enum MemoryStatus: Equatable, Sendable {
    /// Its item waits on the user in the Inbox.
    case waiting(item: String)
    /// Approved: the host appended the text.
    case added(item: String)
    case rejected(item: String)
    /// Its item closed without approve or reject.
    case closed(item: String)

    public var item: String {
        switch self {
        case .waiting(let item), .added(let item), .rejected(let item), .closed(let item): item
        }
    }
}

/// The hash a memory proposal carries: `sha256:` and the hex SHA-256 of `task`, NUL,
/// `title`, NUL, `text` (PROTOCOL.md, "Workspace memory").
public enum MemoryHash {
    public static func of(task: String, title: String, text: String) -> String {
        let digest = SHA256.hash(data: Data((task + "\0" + title + "\0" + text).utf8))
        return "sha256:" + digest.map { String(format: "%02x", $0) }.joined()
    }
}
