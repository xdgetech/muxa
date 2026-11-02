import MuxaProtocol
import MuxaSession
import SwiftUI

/// Triage navigation and the full grouped list share the host's Task records.
struct TasksScreen: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var path: [String] = []
    @State private var listMode = false
    @State private var focused: String?
    @State private var ended = false
    @State private var skipped: Set<String> = []
    @State private var initialized = false
    @State private var newTask: TaskRecordDraft?

    var body: some View {
        NavigationStack(path: $path) {
            VStack {
                if let error = model.session.taskError {
                    Label(error, systemImage: Icon.warning).foregroundStyle(Theme.failure)
                }
                Picker("Inbox mode", selection: $listMode) {
                    Text("Triage").tag(false)
                    Text("List").tag(true)
                }.pickerStyle(.segmented).padding(.horizontal)
                    .accessibilityIdentifier("inbox-mode")
                if !listMode { triage } else {
                    taskList
                }
            }
            .navigationTitle("Inbox")
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
                if !initialized {
                    listMode = model.inboxQueue.isEmpty
                    focused = model.triageOpening ?? model.inboxQueue.first?.id
                    model.triageOpening = nil
                    initialized = true
                }
                if let id = model.tasksOpening {
                    listMode = true
                    model.tasksOpening = nil
                    path = [id]
                }
            }
            .onChange(of: model.inboxQueue.map(\.attentionKey)) { _, _ in
                if let focused, !model.inboxQueue.contains(where: { $0.id == focused }) {
                    self.focused = model.inboxQueue.first?.id
                    ended = model.inboxQueue.isEmpty
                }
            }
            .background {
                Button("Toggle Inbox mode") { listMode.toggle(); path = [] }.keyboardShortcut("l").hidden()
            }
            .refreshable { await reload() }
            // tmux reports changes only for the bridge's session, so states can be stale.
            .task { await reload() }
        }
    }


    private var taskList: some View {
        List {
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
    }

    private var current: InboxItem? { model.inboxQueue.first { $0.id == focused } }

    private func move(_ offset: Int) {
        let queue = model.inboxQueue
        guard !queue.isEmpty else { return }
        if ended, offset < 0 { ended = false; focused = queue.last?.id; return }
        let index = queue.firstIndex { $0.id == focused } ?? 0
        let next = index + offset
        if offset > 0 { skipped.insert(queue[index].id) }
        if next >= queue.count { ended = true }
        else if next >= 0 { focused = queue[next].id; ended = false }
    }

    @ViewBuilder private var triage: some View {
        if ended || model.inboxQueue.isEmpty {
            VStack(spacing: 16) {
                Text("Nothing else needs you.").font(.title2)
                Text("\(skipped.count) skipped · \(model.inboxSnoozedCount) snoozed")
                Button("Previous") { move(-1) }.keyboardShortcut(.upArrow)
                Button("Start over") { ended = false; focused = model.inboxQueue.first?.id; skipped = [] }
                Button("Show List") { listMode = true }
                Button("Show snoozed") { model.inboxSnoozes = [:]; ended = false; focused = model.inboxQueue.first?.id }
            }.frame(maxWidth: .infinity, maxHeight: .infinity).accessibilityIdentifier("triage-end")
        } else if let item = current ?? model.inboxQueue.first {
            ScrollView {
                VStack(alignment: .leading, spacing: 16) {
                    Text("\((model.inboxQueue.firstIndex { $0.id == item.id } ?? 0) + 1) of \(model.inboxQueue.count)")
                    Text(item.group.title).font(.headline)
                    TaskRow(item: item)
                    Text(item.need.title).font(.title2).accessibilityIdentifier("triage-need")
                    Text(item.record.goal).textSelection(.enabled)
                    if let session = item.session, session.defaultPane != nil, model.isConnected {
                        if item.agentPane != nil, model.session.canChat {
                            Button("Open Chat") { model.open(item, as: .chat) }
                        }
                        Button("Open Terminal") { model.open(item, as: .terminal) }
                        if model.session.canReadDocs {
                            Button("TASK.md") { model.open(item, as: .taskFile) }
                            if item.record.loop?.lastCheck != nil {
                                Button("Check log") { model.open(item, as: .checkFile) }
                            }
                            if model.session.canShowChanges, session.worktree != nil {
                                Button("Changes") { model.open(item, as: .changes) }
                            }
                        }
                    }
                    NavigationLink("Task detail", value: item.id)
                    if model.inboxSnoozedCount > 0 {
                        Button("Show snoozed (\(model.inboxSnoozedCount))") { model.inboxSnoozes = [:] }
                    }
                    ForEach(Array(model.inboxQueue.drop(while: { $0.id != item.id }).dropFirst().prefix(2))) { next in
                        Button("Up next: \(next.record.title) · \(next.need.title)") { focused = next.id }
                    }
                }.padding()
            }.accessibilityIdentifier("triage-card")
            .safeAreaInset(edge: .bottom) {
                triageControls(item).padding().background(.regularMaterial)
            }
            .simultaneousGesture(DragGesture(minimumDistance: 60).onEnded { if $0.translation.width < -60 { move(1) } })
        }
    }

    private func triageControls(_ item: InboxItem) -> some View {
        HStack {
            Button("Previous") { move(-1) }.keyboardShortcut(.upArrow)
            Button("Snooze") {
                model.inboxSnoozes[item.id] = item.attentionKey
                focused = model.inboxQueue.first?.id
            }.keyboardShortcut("s").accessibilityIdentifier("triage-snooze")
            Button("Next") { move(1) }.keyboardShortcut(.downArrow).accessibilityIdentifier("triage-next")
            Button("Open") {
                model.open(item, as: item.agentPane != nil && model.session.canChat ? .chat : .terminal)
            }.keyboardShortcut("o").disabled(item.session?.defaultPane == nil || !model.isConnected)
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
