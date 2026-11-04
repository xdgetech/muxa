import MuxaProtocol
import MuxaSession
import SwiftUI

/// A task's title, where it belongs, its agent's state with its age, and its loop.
struct TaskRow: View {
    let item: MuxaSession.InboxItem

    var body: some View {
        HStack {
            VStack(alignment: .leading, spacing: 2) {
                Text(item.record.title).font(.headline)
                Text(TaskDetail.place(item.record)).font(.caption).foregroundStyle(.secondary)
            }
            Spacer()
            VStack(alignment: .trailing, spacing: 2) {
                if let pane = item.agentPane {
                    AgentBadge(pane: pane)
                } else {
                    Text(item.session == nil ? "no session" : "no agent").font(.caption).foregroundStyle(.secondary)
                }
                if item.record.status != .open {
                    Text(TaskDetail.title(item.record.status)).font(.caption).foregroundStyle(.secondary)
                }
                LoopBadge(record: item.record)
            }
        }
    }
}

extension InboxNeed {
    var title: String {
        switch self {
        case .none: "Nothing needs you"
        case .answer: "Waits for your answer"
        case .agentError: "The agent stopped with an error"
        case .loop: "Loop needs attention"
        case .review: "Ready for review"
        case .noAgent: "No agent state is available"
        case .noSession: "No session runs this Task"
        }
    }
}
