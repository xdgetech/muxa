import MuxaV2
import SwiftUI

/// Opens an empty Start Task sheet; `⌘⇧N` (the Mac's File menu has the key).
struct NewTaskButton: View {
    @Environment(NextNavigation.self) private var nav
    let id: String
    /// The Workspace the sheet starts with.
    var workspace: String?

    var body: some View {
        Button("New Task", systemImage: "plus") { nav.newTask = TaskSeed(workspace: workspace) }
            .touchShortcut("n")
            .accessibilityIdentifier(id)
    }
}

/// Start Task (ADR 0068): a Workspace, a title and a goal, the places, the
/// connections and MCP servers, plan mode and a Task Loop. The started
/// agent opens in Focus.
struct StartTaskSheet: View {
    @Environment(NextModel.self) private var model
    let seed: TaskSeed
    /// The started Task's agent, or nil when the user cancelled.
    var onClose: (MuxaV2.Terminal?) -> Void
    @State private var workspaces: [Workspace]?
    @State private var draft: TaskDraft
    @State private var error: String?
    @State private var starting = false

    init(seed: TaskSeed, onClose: @escaping (MuxaV2.Terminal?) -> Void) {
        self.seed = seed
        self.onClose = onClose
        _draft = State(initialValue: TaskDraft(seed: seed))
    }

    var body: some View {
        NavigationStack {
            Form {
                if let ws = current {
                    task(ws)
                    TaskAgentSection(draft: $draft, workspace: ws)
                    TaskPlacesSection(draft: $draft, workspace: ws)
                    TaskToolsSection(draft: $draft, workspace: ws)
                } else if workspaces?.isEmpty == true {
                    Text("The host has no Workspaces. Add one in workspaces.toml on the host.")
                        .foregroundStyle(.secondary)
                } else if workspaces == nil, error == nil {
                    ProgressView()
                }
                if let error {
                    Text(error).foregroundStyle(.red).accessibilityIdentifier("task.error")
                }
            }
            .formStyle(.grouped)
            .navigationTitle("New Task")
            .inlineTitle()
            .toolbar { toolbar }
            .task { await load() }
        }
        #if os(macOS)
        .frame(minWidth: 460, minHeight: 560)
        #endif
    }

    private var current: Workspace? { workspaces?.first { $0.name == draft.workspace } }

    private func task(_ ws: Workspace) -> some View {
        Section {
            Picker("Workspace", selection: workspaceChoice) {
                ForEach(workspaces ?? []) { Text($0.name).tag($0.name) }
            }
            .accessibilityIdentifier("task.workspace")
            TextField("Title", text: $draft.title)
                .accessibilityIdentifier("task.title")
            TextField("Goal", text: $draft.goal, axis: .vertical)
                .lineLimit(1...4)
                .accessibilityIdentifier("task.goal")
        } footer: {
            if let missing = draft.missing(in: ws) { Text(missing) }
        }
    }

    private var workspaceChoice: Binding<String> {
        Binding(
            get: { draft.workspace },
            set: { name in
                if let ws = workspaces?.first(where: { $0.name == name }) { draft.choose(ws) }
            })
    }

    @ToolbarContentBuilder
    private var toolbar: some ToolbarContent {
        ToolbarItem(placement: .cancellationAction) {
            Button("Cancel") { onClose(nil) }
        }
        ToolbarItem(placement: .confirmationAction) {
            Button("Start") { Task { await start() } }
                .disabled(starting || current.map { draft.missing(in: $0) != nil } ?? true)
                .accessibilityIdentifier("task.start")
        }
    }

    private func load() async {
        do {
            let list = try await model.workspaces()
            workspaces = list
            let pick = list.first { $0.name.lowercased() == seed.workspace?.lowercased() } ?? list.first
            if let pick { draft.choose(pick) }
        } catch {
            self.error = NextModel.describe(error)
        }
    }

    private func start() async {
        guard let ws = current else { return }
        starting = true
        defer { starting = false }
        do {
            error = nil
            onClose(try await model.startTask(draft.request(in: ws)))
        } catch {
            self.error = NextModel.describe(error)
        }
    }
}
