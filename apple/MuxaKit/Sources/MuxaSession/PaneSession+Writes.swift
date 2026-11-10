import CryptoKit
import Foundation
import MuxaProtocol

/// Writes outside the host that a Result proposes (ADR 0052) on a host with `work` level 13.
/// Approve carries the hash of the write the screen showed whole; nothing else approves one.
extension PaneSession {
    public var canUseWrites: Bool { workLevel >= workWrites }

    /// Shown when the host cannot perform an approved write.
    public static let writesUnsupported = "This host cannot perform writes. Update muxa on the host."
    /// Shown when approve does not come from the screen that shows the whole write, or the
    /// shown write is not the listed one.
    public static let writeNotShown = "Open the write and read its whole content before you approve it."

    /// The worker that performs the workspace's approved writes. Nil below level 13.
    public func writer(of workspace: String?) -> WriterInfo? {
        guard canUseWrites, let workspace else { return nil }
        return assistantInfo?.delegates.first { $0.workspace == workspace }?.writer
    }

    /// Whether the workspace takes write proposals. Nil below level 13 and for a workspace
    /// the host does not list.
    public func writesMode(of workspace: String?) -> Writes? {
        guard canUseWrites, let workspace else { return nil }
        return assistantInfo?.delegates.first { $0.workspace == workspace }?.writes
    }

    /// Approves a `write` item. `shown` is the write as the screen showed it whole; its
    /// hash, computed again here from what was shown, is what the host checks. A write
    /// that differs from the listed item, or whose text does not hash to its `hash`, is
    /// refused here and nothing is sent.
    @discardableResult
    public func approveWrite(_ item: InboxItem, shown: WriteProposal) async throws -> InboxItem {
        guard canUseWrites else {
            throw Failure.bridge(ErrorBody(code: .unsupported, message: Self.writesUnsupported))
        }
        let listed = inboxItems.first { $0.id == item.id }?.write ?? item.write
        let hash = WriteHash.of(action: shown.action, target: shown.target, content: shown.content)
        guard item.kind == .write, listed == shown, hash == shown.hash else {
            throw Failure.bridge(ErrorBody(code: .conflict, message: Self.writeNotShown))
        }
        return try await respond(item, .approve, text: nil, option: nil, worktree: .remove, hash: hash)
    }

    /// The `write` item that asks about `write`: by the id the Result names, else by its hash.
    public func writeItem(for write: ResultWrite, of request: AssistantRequest) -> InboxItem? {
        if let id = write.item, let item = inboxItems.first(where: { $0.id == id }) { return item }
        return inboxItems.first { $0.kind == .write && $0.write?.request == request.id && $0.write?.hash == write.hash }
    }

    /// What became of a proposed write, as the host records it. It never says the write
    /// happened: the writer's Task reports that (ADR 0052 §3).
    public func writeStatus(_ write: ResultWrite, of request: AssistantRequest) -> WriteStatus {
        let item = writeItem(for: write, of: request)
        if let task = write.task ?? item?.task { return .taskStarted(task) }
        guard let item else { return .proposed }
        switch item.state {
        case .open, .later: return .waiting(item: item.id)
        case .answered where item.answer == .reject: return .rejected
        case .answered, .resolved, .other: return .closed
        }
    }

    /// The request whose Result proposed the write item's write.
    public func request(of write: WriteProposal) -> AssistantRequest? {
        assistantRequests.first { $0.id == write.request }
    }
}

/// A proposed write's state in Work detail.
public enum WriteStatus: Equatable, Sendable {
    /// The host made no item for it yet.
    case proposed
    /// Its item waits on the user.
    case waiting(item: String)
    case rejected
    /// Its item closed without approve or reject.
    case closed
    /// Approved: the writer's Task.
    case taskStarted(String)
}

/// The hash a write proposal carries: `sha256:` and the hex SHA-256 of `action`, NUL,
/// `target`, NUL, `content` (PROTOCOL.md, "Write proposals").
public enum WriteHash {
    public static func of(action: String, target: String, content: String) -> String {
        let digest = SHA256.hash(data: Data((action + "\0" + target + "\0" + content).utf8))
        return "sha256:" + digest.map { String(format: "%02x", $0) }.joined()
    }
}

extension WriteProposal {
    /// From an older version of its request than `current`.
    public func isOlder(than current: AssistantRequest?) -> Bool {
        guard let current else { return false }
        return version < current.version
    }
}
