import Foundation
import MuxaProtocol
import MuxaSession

/// Writes outside the host that a Result proposes (ADR 0052), on a host with `work` level 13.
/// Only the item screen approves one, after it showed the content whole.
extension AppModel {
    var usesWrites: Bool { usesItems && session.canUseWrites }

    /// The item screen shows this write whole and may offer Approve.
    func showsWrite(_ item: InboxItem) -> Bool { usesWrites && item.kind == .write && item.write != nil }

    /// The request whose Result proposed the item's write: a newer version marks it older.
    func isOlderWrite(_ write: WriteProposal) -> Bool {
        write.isOlder(than: session.request(of: write))
    }

    /// "Task started", then the Task's own state; the app never says the write happened.
    func shown(_ status: WriteStatus) -> String {
        switch status {
        case .proposed: "Proposed"
        case .waiting: "Waiting for you in the Inbox"
        case .rejected: "Rejected"
        case .closed: "Closed without an answer"
        case .taskStarted(let task):
            if let record = session.taskRecords.first(where: { $0.id == task }) {
                "Task started · \(TaskDetail.title(record.status))"
            } else {
                "Task started"
            }
        }
    }
}
