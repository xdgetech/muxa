import MuxaProtocol
import MuxaSession
import SwiftUI

/// Where the Inbox sheet's stack goes.
enum InboxRoute: Hashable {
    case task(String)
    case entry(String)
    case routines
    case routine(workspace: String, name: String)
}

/// The Inbox sheet: the Inbox list, and All Tasks grouped by need with the Routines.
struct TasksScreen: View {
    enum Mode: Hashable { case inbox, tasks }

    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var path: [InboxRoute] = []
    @State private var mode = Mode.inbox
    @State private var focused: String?
    @State private var initialized = false
    @State private var newTask: TaskRecordDraft?

    var body: some View {
        NavigationStack(path: $path) {
            VStack {
                if let error = model.session.taskError {
                    Label(error, systemImage: Icon.warning).foregroundStyle(Theme.failure)
                }
                Picker("Inbox mode", selection: $mode) {
                    Text("Inbox").tag(Mode.inbox)
                    Text("All Tasks").tag(Mode.tasks)
                }.pickerStyle(.segmented).padding(.horizontal)
                    .accessibilityIdentifier("inbox-mode")
                if mode == .inbox {
                    InboxListView(focused: $focused, path: $path)
                } else {
                    taskList
                }
            }
            .navigationTitle("Inbox")
            .navigationBarTitleDisplayMode(.inline)
            .navigationDestination(for: InboxRoute.self) { route in
                switch route {
                case .task(let id): TaskDetail(id: id)
                case .entry(let id): InboxEntryDetail(id: id, path: $path)
                case .routines: RoutinesScreen()
                case .routine(let workspace, let name): RoutineDetail(workspace: workspace, name: name)
                }
            }
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
                    CreateTaskSheet(draft: draft) { record in path = [.task(record.id)] }
                }
            }
            .onAppear(perform: opened)
            .background {
                Button("Toggle Inbox mode") {
                    mode = mode == .inbox ? .tasks : .inbox
                    path = []
                }.keyboardShortcut("l").hidden()
            }
            .refreshable { await reload() }
            // tmux reports changes only for the bridge's session, so states can be stale.
            .task { await reload() }
        }
    }

    /// The Inbox when something needs the user, else All Tasks; or where the app asked.
    private func opened() {
        if !initialized {
            mode = model.inboxRows.isEmpty ? .tasks : .inbox
            focused = model.triageOpening ?? model.inboxRows.first?.id
            model.triageOpening = nil
            initialized = true
        }
        if let id = model.tasksOpening {
            mode = .tasks
            model.tasksOpening = nil
            path = [.task(id)]
        }
        if model.routinesOpening {
            mode = .tasks
            model.routinesOpening = false
            path = [.routines]
        }
    }

    private var taskList: some View {
        List {
            if model.canUseRoutines {
                Section {
                    NavigationLink(value: InboxRoute.routines) {
                        LabeledContent {
                            Text("\(model.session.routines.count)")
                        } label: {
                            Label("Routines", systemImage: Icon.routine)
                        }
                    }
                    .accessibilityIdentifier("open-routines")
                }
            }
            ForEach(model.inbox) { section in
                Section {
                    ForEach(section.items) { item in
                        NavigationLink(value: InboxRoute.task(item.id)) { TaskRow(item: item) }
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
        .accessibilityIdentifier("all-tasks")
        .overlay {
            if model.inbox.isEmpty, model.session.taskError == nil {
                ContentUnavailableView("No Tasks", systemImage: Icon.tasks,
                                       description: Text("Tasks you start on this host show here, with their agent's state."))
            }
        }
    }

    private func reload() async {
        try? await model.session.refresh()
        await model.session.loadTasks()
        await model.session.loadWork()
    }
}
