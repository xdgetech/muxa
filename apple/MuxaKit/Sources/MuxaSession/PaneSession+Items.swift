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
    public static let itemConflict = "The agent now asks something else. The Inbox shows its latest question. Try again."

    /// Answers `item` and returns it as the host saved it. A refusal lists the Inbox
    /// again, because another device or the terminal may have answered first.
    @discardableResult
    public func answer(
        _ item: InboxItem, _ action: InboxAction, text: String? = nil, option: UInt32? = nil, until: UInt64? = nil
    ) async throws -> InboxItem {
        guard canUseItems else {
            throw Failure.bridge(ErrorBody(code: .unsupported, message: Self.itemsUnsupported))
        }
        var requestId: UInt64 = 0
        do {
            try await request { id in
                requestId = id
                return .updateInboxEntry(id: id, entry: item.id, action: action, until: until, text: text, option: option)
            }
        } catch Failure.bridge(let body) where body.code == .conflict {
            savedItems[requestId] = nil
            await loadWork()
            throw Failure.bridge(ErrorBody(code: .conflict, message: Self.itemConflict))
        } catch {
            savedItems[requestId] = nil
            await loadWork()
            throw error
        }
        guard let saved = savedItems.removeValue(forKey: requestId) else {
            throw Failure.bridge(ErrorBody(code: .internal, message: "The bridge did not return the Inbox item."))
        }
        // Accept, Drop, Retry and an approved proposal change a Task.
        if action != .read, action != .later { await loadTasks() }
        return saved
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
