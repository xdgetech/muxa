import MuxaProtocol
import MuxaSession
import SwiftUI

/// A Task's detail over its pane, from More (ADR 0044).
struct TaskInfoSheet: View {
    @Environment(AppModel.self) private var model
    let id: String

    var body: some View {
        NavigationStack {
            TaskDetail(id: id)
                .navigationTitle(model.inboxItem(id)?.record.title ?? "Task")
                .navigationBarTitleDisplayMode(.inline)
                .toolbar {
                    ToolbarItem(placement: .confirmationAction) {
                        Button("Done") { model.taskInfo = nil }.accessibilityIdentifier("task-info-done")
                    }
                }
        }
        .accessibilityIdentifier("task-info")
    }
}

/// One task record: its goal, where its agent works, and the ways into its session.
struct TaskDetail: View {
    @Environment(AppModel.self) private var model
    let id: String
    @State private var saving = false
    @State private var error: String?
    @State private var lostWork: LostWork?
    @State private var loopStart: LoopStart?
    @State private var loopError: String?

    var body: some View {
        if let item = model.inboxItem(id) {
            form(item)
        } else {
            ContentUnavailableView("Task Not Found", systemImage: Icon.tasks,
                                   description: Text("The host no longer lists this task."))
        }
    }

    private func form(_ item: TaskItem) -> some View {
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
            RestartRecoverySection(record: r)
            LoopSection(item: item, starting: $loopStart, error: $loopError)
            Section {
                statusPicker(r)
                LabeledContent("Agent") {
                    if let pane = item.agentPane {
                        AgentBadge(pane: pane)
                            .accessibilityElement(children: .combine)
                            .accessibilityIdentifier("task-agent-state")
                    } else { Text("none") }
                }
                LabeledContent("Workspace", value: r.workspace)
                if let memory = model.memoryShown(of: r.workspace) {
                    LabeledContent("Workspace Memory", value: memory)
                        .accessibilityIdentifier("task-workspace-memory")
                }
                if model.session.canUseWorkers || r.worker != nil {
                    LabeledContent("Worker", value: r.worker ?? "Workspace default")
                }
                if let project = r.project { LabeledContent("Project", value: project) }
            }
            TaskMemorySection(task: r.id)
            Section("Where") {
                if let session = item.session?.name ?? r.session { LabeledContent("Session", value: session) }
                if let repo = r.repo { LabeledContent("Repository", value: repo) }
                if let branch = r.branch { LabeledContent("Branch", value: branch) }
                if let dir = r.dir { LabeledContent("Directory", value: dir) }
            }
            .font(.callout)
            TaskRunsSection(item: item)
        }
        .accessibilityIdentifier("task-detail")
        .navigationTitle(r.title)
        .navigationBarTitleDisplayMode(.inline)
        .loopSheets(starting: $loopStart, error: $loopError)
        .lostWorkAlert($lostWork)
        .alert("Can't Change the Status", isPresented: Binding(get: { error != nil }, set: { if !$0 { error = nil } })) {
            Button("OK", role: .cancel) {}
        } message: {
            Text(error ?? "")
        }
    }

    /// The four statuses a user sets; one from a newer bridge is shown as it is.
    private func statusPicker(_ r: TaskRecord) -> some View {
        Picker("Status", selection: Binding(get: { r.status }, set: { set($0, on: r) })) {
            ForEach(Self.statuses + (Self.statuses.contains(r.status) ? [] : [r.status]), id: \.self) { status in
                Text(Self.title(status)).tag(status)
            }
        }
        .pickerStyle(.menu)
        .disabled(saving)
        .accessibilityIdentifier("task-status")
    }

    private static let statuses: [TaskStatus] = [.open, .review, .done, .dropped]

    /// Done and Drop remove the worktree; when that could lose work, the user decides.
    private func set(_ status: TaskStatus, on r: TaskRecord, worktree: WorktreeEnd = .remove) {
        guard status != r.status, !saving else { return }
        saving = true
        Task {
            defer { saving = false }
            do {
                _ = try await model.session.updateTask(r, status: status, worktree: worktree)
            } catch {
                if let reason = PaneSession.lostWork(error) {
                    lostWork = LostWork(reason: reason) { set(status, on: r, worktree: .force) }
                } else {
                    self.error = PaneSession.describe(error)
                }
            }
        }
    }

    @ViewBuilder
    private func open(_ item: TaskItem) -> some View {
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
                    if model.session.canRestartAgents, item.hasHandoff {
                        Button("Read HANDOFF.md", systemImage: Icon.document) { model.open(item, as: .handoffFile) }
                            .accessibilityIdentifier("task-open-handoff")
                    }
                    if model.session.canShowChanges {
                        Button("Changes", systemImage: Icon.changes) { model.open(item, as: .changes) }
                            .accessibilityIdentifier("task-open-changes")
                    }
                }
                if model.session.canRestartAgents { RestartAgentButton(item: item) }
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
