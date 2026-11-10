import MuxaProtocol
import MuxaSession
import SwiftUI

/// A `memory` item's proposal (ADR 0053 §7): the text whole, as plain text in a fixed-width
/// font. `Text(verbatim:)` renders no Markdown and opens no link, so the text the user
/// approves is the text the user saw.
struct MemoryProposalView: View {
    @Environment(AppModel.self) private var model
    let item: InboxItem
    let memory: MemoryProposal

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            LabeledContent("Workspace", value: item.workspace)
                .accessibilityIdentifier("item-memory-workspace")
            Text("Text").font(.caption.weight(.semibold)).foregroundStyle(.secondary)
            Text(verbatim: memory.text)
                .font(.callout.monospaced())
                .textSelection(.enabled)
                .fixedSize(horizontal: false, vertical: true)
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(10)
                .background(.background, in: RoundedRectangle(cornerRadius: 8))
                .accessibilityIdentifier("item-memory-text")
            // Over 4 KiB the host offers no Approve and says why here.
            if let summary = item.summary, !summary.isEmpty {
                Text(verbatim: summary).font(.callout).foregroundStyle(.secondary).textSelection(.enabled)
                    .accessibilityIdentifier("item-memory-summary")
            }
            if model.showsMemory(item), item.answers.contains(.approve) {
                Text("Approve adds this text, unchanged, to .muxa/WORKSPACE.md of \(item.workspace). "
                     + "Every later Task in the workspace reads it.")
                    .font(.caption).foregroundStyle(.secondary)
            }
            if model.inboxItem(memory.task) != nil {
                Button("Open the Task", systemImage: Icon.tasks) { model.select(.task(memory.task)) }
                    .accessibilityIdentifier("item-memory-task")
            }
        }
    }
}

/// The Task detail's line after the end (ADR 0053 §7). Tapping a waiting proposal opens
/// its item.
struct TaskMemorySection: View {
    @Environment(AppModel.self) private var model
    let task: String

    var body: some View {
        if model.usesMemory, let status = model.session.memoryStatus(of: task) {
            Section {
                if case .waiting(let item) = status {
                    Button(model.shown(status), systemImage: Icon.note) { model.select(.item(item)) }
                        .accessibilityIdentifier("task-memory")
                } else {
                    Label(model.shown(status), systemImage: Icon.note)
                        .accessibilityElement(children: .combine)
                        .accessibilityIdentifier("task-memory")
                }
            }
        }
    }
}

/// The workspace's memory mode and its file's last change, beside the staff (ADR 0053 §7).
struct WorkspaceMemorySection: View {
    @Environment(AppModel.self) private var model
    let workspace: String?

    var body: some View {
        if let shown = model.memoryShown(of: workspace) {
            Section {
                LabeledContent("Workspace Memory", value: shown)
                    .accessibilityIdentifier("workspace-memory")
            } footer: {
                Text(footer)
            }
        }
    }

    private var footer: String {
        switch model.session.memoryMode(of: workspace) {
        case .review?: "A Task's end proposes what it learned. You approve each addition in the Inbox."
        case .auto?: "A Task's end adds what it learned at once, within the file's limit."
        case .off?: "Tasks add nothing to the workspace memory."
        case .other?, nil: ""
        }
    }
}
