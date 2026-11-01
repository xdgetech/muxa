import MuxaProtocol
import MuxaSession
import SwiftUI

/// One task record: its goal, where its agent works, and the ways into its session.
struct TaskDetail: View {
    @Environment(AppModel.self) private var model
    let id: String

    var body: some View {
        if let item = model.inboxItem(id) {
            form(item)
        } else {
            ContentUnavailableView("Task Not Found", systemImage: Icon.tasks,
                                   description: Text("The host no longer lists this task."))
        }
    }

    private func form(_ item: InboxItem) -> some View {
        let r = item.record
        return Form {
            if let note = r.note {
                Section {
                    Label(note, systemImage: Icon.warning)
                        .foregroundStyle(Theme.attention)
                        .accessibilityIdentifier("task-note")
                }
            }
            Section("Goal") {
                Text(r.goal).textSelection(.enabled)
            }
            // Before the details: a long goal must not push the way into the session off screen.
            open(item)
            Section {
                LabeledContent("Status", value: Self.title(r.status))
                LabeledContent("Agent") {
                    if let pane = item.agentPane { AgentBadge(pane: pane) } else { Text("none") }
                }
                LabeledContent("Workspace", value: r.workspace)
                if let project = r.project { LabeledContent("Project", value: project) }
            }
            Section("Where") {
                if let session = item.session?.name ?? r.session { LabeledContent("Session", value: session) }
                if let repo = r.repo { LabeledContent("Repository", value: repo) }
                if let branch = r.branch { LabeledContent("Branch", value: branch) }
                if let dir = r.dir { LabeledContent("Directory", value: dir) }
            }
            .font(.callout)
        }
        .navigationTitle(r.title)
        .navigationBarTitleDisplayMode(.inline)
    }

    @ViewBuilder
    private func open(_ item: InboxItem) -> some View {
        if item.session != nil {
            Section {
                if item.agentPane != nil, model.session.canChat {
                    Button("Chat", systemImage: Icon.chat) { model.open(item, as: .chat) }
                        .accessibilityIdentifier("task-open-chat")
                }
                Button("Terminal", systemImage: Icon.agent) { model.open(item, as: .terminal) }
                    .accessibilityIdentifier("task-open-terminal")
                if model.session.canReadDocs {
                    Button("Read TASK.md", systemImage: Icon.document) { model.open(item, as: .taskFile) }
                        .accessibilityIdentifier("task-open-file")
                    if model.session.canShowChanges {
                        Button("Changes", systemImage: Icon.changes) { model.open(item, as: .changes) }
                            .accessibilityIdentifier("task-open-changes")
                    }
                }
            } header: {
                Text("Open")
            }
        } else {
            Section {
                Text("No session runs this task.").foregroundStyle(.secondary)
            }
        }
    }

    /// "work · Billing": the workspace, and the project when there is one.
    static func place(_ r: TaskRecord) -> String {
        [r.workspace, r.project].compactMap { $0 }.joined(separator: " · ")
    }

    static func title(_ status: TaskStatus) -> String {
        switch status {
        case .open: "Open"
        case .review: "In review"
        case .done: "Done"
        case .dropped: "Dropped"
        case .other(let raw): raw
        }
    }
}
