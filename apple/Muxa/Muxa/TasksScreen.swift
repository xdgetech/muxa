import MuxaProtocol
import MuxaSession
import SwiftUI

/// The host's task records as an Inbox: what needs the user first. A task opens
/// its detail, and from there its session. The sessions list stays as it is.
struct TasksScreen: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var path: [String] = []
    @State private var newTask: TaskRecordDraft?

    var body: some View {
        NavigationStack(path: $path) {
            List {
                if let error = model.session.taskError {
                    Label(error, systemImage: Icon.warning).foregroundStyle(Theme.failure)
                }
                ForEach(model.inbox) { section in
                    Section {
                        ForEach(section.items) { item in
                            NavigationLink(value: item.id) { TaskRow(item: item) }
                                .accessibilityIdentifier("task-\(item.id)")
                        }
                    } header: {
                        Text(section.group.title).accessibilityIdentifier("inbox-\(section.group.title)")
                    } footer: {
                        if section.group == model.inbox.last?.group, model.session.tasksTruncated {
                            Text("The host has more tasks than it listed.")
                        }
                    }
                }
            }
            .overlay {
                if model.inbox.isEmpty, model.session.taskError == nil {
                    ContentUnavailableView("No Tasks", systemImage: Icon.tasks,
                                           description: Text("Tasks you start on this host show here, with their agent's state."))
                }
            }
            .navigationTitle("Tasks")
            .navigationBarTitleDisplayMode(.inline)
            .navigationDestination(for: String.self) { id in TaskDetail(id: id) }
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Close") { dismiss() }.keyboardShortcut(.cancelAction)
                }
                // The host's config names the workspaces; without one, nothing can be created.
                if let first = model.session.workspaces.first {
                    ToolbarItem(placement: .primaryAction) {
                        Button("New Task", systemImage: Icon.add) { newTask = TaskRecordDraft(workspace: first.name) }
                            .keyboardShortcut("n")
                            .accessibilityIdentifier("record-new")
                    }
                }
            }
            .sheet(isPresented: Binding(get: { newTask != nil }, set: { if !$0 { newTask = nil } })) {
                if let draft = newTask {
                    CreateTaskSheet(draft: draft) { record in path = [record.id] }
                }
            }
            .onAppear {
                if let id = model.tasksOpening {
                    model.tasksOpening = nil
                    path = [id]
                }
            }
            .refreshable { await reload() }
            // tmux reports changes only for the bridge's session, so states can be stale.
            .task { await reload() }
        }
    }

    private func reload() async {
        try? await model.session.refresh()
        await model.session.loadTasks()
    }
}

/// A task's title, where it belongs, its agent's state with its age, and its loop.
struct TaskRow: View {
    let item: InboxItem

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
