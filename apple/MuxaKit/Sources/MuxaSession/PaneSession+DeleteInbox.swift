import Foundation
import MuxaProtocol

/// Protect and Delete Old (ADR 0046), on a host with `work` level 8.
extension PaneSession {
    public var canDeleteInbox: Bool { workLevel >= workDeleteInbox }

    /// The ages Delete Old offers, in days.
    public static let deleteOldDays: [UInt32] = [1, 3, 5]

    /// Shown when the host keeps no protected items.
    public static let deleteInboxUnsupported = "This host cannot protect or delete Inbox items. Update muxa on the host."

    /// Protects `item`, or clears its mark, and returns it as the host saved it. It
    /// is no answer: the item keeps its place and its answers.
    @discardableResult
    public func setProtected(_ item: InboxItem, _ protected: Bool) async throws -> InboxItem {
        try requireDeleteInbox()
        return try await answer(item, protected ? .protect : .unprotect)
    }

    /// How many listed items Delete Old would remove, for its confirmation.
    public func deletableCount(olderThanDays days: UInt32, now: UInt64 = UInt64(Date().timeIntervalSince1970)) -> Int {
        inboxItems.filter { $0.isDeletable(olderThanDays: days, now: now) }.count
    }

    /// Removes every unprotected item made more than `days` days ago, open or closed,
    /// then lists the Inbox again. Returns how many the host removed.
    @discardableResult
    public func deleteOld(olderThanDays days: UInt32) async throws -> UInt32 {
        try requireDeleteInbox()
        var requestId: UInt64 = 0
        do {
            try await request { id in
                requestId = id
                return .deleteInbox(id: id, olderThanDays: days)
            }
        } catch {
            deletedCounts[requestId] = nil
            throw error
        }
        let deleted = deletedCounts.removeValue(forKey: requestId)
        await loadWork()
        guard let deleted else {
            throw Failure.bridge(ErrorBody(code: .internal, message: "The bridge did not say how many items it deleted."))
        }
        return deleted
    }

    /// The confirmation's text (docs/inbox.md): the count, or that there is nothing to delete.
    public nonisolated static func deleteOldMessage(count: Int, days: UInt32) -> String {
        let age = days == 1 ? "1 day" : "\(days) days"
        if count == 0 { return "No unprotected items are older than \(age)." }
        return "Delete \(count) \(count == 1 ? "item" : "items") older than \(age)? Protected items stay."
    }

    private func requireDeleteInbox() throws {
        guard canDeleteInbox else {
            throw Failure.bridge(ErrorBody(code: .unsupported, message: Self.deleteInboxUnsupported))
        }
    }
}
