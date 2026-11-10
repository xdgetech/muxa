import Foundation
import MuxaProtocol

/// The Inbox as a list of questions (ADR 0033): items the host keeps at `work` level 3.
extension PaneSession {
    public var canUseItems: Bool { workLevel >= workItems }

    /// The items that wait on the user, in the host's order: `ask` first, then newest.
    public var openItems: [InboxItem] { inboxItems.filter { $0.state == .open } }

    /// Shown when the host keeps no Inbox items.
    public static let itemsUnsupported = "This host keeps no Inbox items. Update muxa on the host."
    /// Shown when the agent's menu changed since the item was made. Nothing reached the pane.
    /// Shown when the host cannot send an item to the workspace's agent.
    public static let askAgentUnsupported = "This host cannot send an item to the workspace's agent. Update muxa on the host."
    public static let itemConflict = "The agent now asks something else. The Inbox shows its latest question. Try again."

    /// Answers `item` and returns it as the host saved it. A refusal lists the Inbox
    /// again, because another device or the terminal may have answered first.
    /// Complete Task and Drop Task take `worktree` as `updateTask` does.
    @discardableResult
    public func answer(
        _ item: InboxItem, _ action: InboxAction, text: String? = nil, option: UInt32? = nil,
        worktree: WorktreeEnd = .remove
    ) async throws -> InboxItem {
        guard canUseItems else {
            throw Failure.bridge(ErrorBody(code: .unsupported, message: Self.itemsUnsupported))
        }
        let end = action == .accept || action == .drop ? worktreeEnd(worktree) : nil
        // The overall assistant's proposals (ADR 0049 §4): approve sorts a Capture or asks a
        // delegate, and a `capture-saved` or `work-saved` comes before `inbox-saved`.
        let proposes = item.kind == .sort || item.kind == .delegate
        var requestId: UInt64 = 0
        defer {
            savedCaptures[requestId] = nil
            savedWork[requestId] = nil
            savedRequests[requestId] = nil
        }
        do {
            try await request { id in
                requestId = id
                return .updateInboxEntry(
                    id: id, entry: item.id, action: action, until: nil, text: text, option: option, worktree: end)
            }
        } catch Failure.bridge(let body) where body.code == .conflict && !proposes {
            savedItems[requestId] = nil
            await loadWork()
            throw Failure.bridge(ErrorBody(code: .conflict, message: Self.itemConflict))
        } catch {
            // A proposal's refusal keeps the host's reason: a sorted Capture, a budget, no daemon.
            savedItems[requestId] = nil
            await loadWork()
            throw error
        }
        guard let saved = savedItems.removeValue(forKey: requestId) else {
            throw Failure.bridge(ErrorBody(code: .internal, message: "The bridge did not return the Inbox item."))
        }
        switch action {
        case .read, .protect, .unprotect: break
        // The host moved the item down (ADR 0040): its order is the list's.
        case .later: await loadWork()
        // An approved proposal sorted a Capture or made a request: the Work list changed.
        case _ where proposes: await loadWork()
        // Accept, Drop, Retry and an approved proposal change a Task.
        default: await loadTasks()
        }
        return saved
    }

    /// Sends `item` and `text` to the workspace's Ask session (ADR 0041), which the
    /// host starts when none is open, and lists sessions again so the list has it.
    /// The item stays open. Returns the session.
    public func askAgent(_ item: InboxItem, text: String) async throws -> StartedSession {
        guard canUseItems, item.answers.contains(.askAgent) else {
            throw Failure.bridge(ErrorBody(code: .unsupported, message: Self.askAgentUnsupported))
        }
        var requestId: UInt64 = 0
        do {
            try await request { id in
                requestId = id
                return .updateInboxEntry(id: id, entry: item.id, action: .askAgent, until: nil, text: text, option: nil)
            }
        } catch {
            savedItems[requestId] = nil
            startedSessions[requestId] = nil
            throw error
        }
        savedItems[requestId] = nil
        guard let started = startedSessions.removeValue(forKey: requestId) else {
            throw Failure.bridge(ErrorBody(code: .internal, message: "The bridge did not name the Ask session."))
        }
        try await refresh()
        return started
    }

    /// Marks `item` read once a device opens it. Read is a hint, not an answer:
    /// a failure leaves the dot and is no error to show (an older host refuses
    /// it on a Routine's entry).
    public func markRead(_ item: InboxItem) async {
        guard canUseItems, item.readAt == nil else { return }
        _ = try? await answer(item, .read)
    }

    /// A newer state of a listed item replaces it; a new one goes first.
    func keep(_ item: InboxItem) {
        if let i = inboxItems.firstIndex(where: { $0.id == item.id }) {
            inboxItems[i] = item
        } else {
            inboxItems.insert(item, at: 0)
        }
    }
}
