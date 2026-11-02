import MuxaProtocol
import MuxaSession
import SwiftUI

/// Records a Task on the host and starts an agent for it: in a directory of its
/// workspace, or with a repo, on a branch in a worktree.
struct CreateTaskSheet: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State var draft: TaskRecordDraft
    /// Called with the record the host kept, also when its agent did not start.
    let created: (TaskRecord) -> Void
    @State private var withRepo = false
    @State private var creating = false
    @State private var error: String?
    @FocusState private var titleFocused: Bool

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    Picker("Workspace", selection: $draft.workspace) {
                        ForEach(model.session.workspaces) { w in
                            Text(w.name).tag(w.name)
                        }
                    }
                    .accessibilityIdentifier("record-workspace")
                    TextField("Title", text: $draft.title)
                        .focused($titleFocused)
                        .accessibilityIdentifier("record-title")
                    TextField("Project (optional)", text: $draft.project)
                        .accessibilityIdentifier("record-project")
                }
                Section {
                    TextField("What should the agent do?", text: $draft.goal, axis: .vertical)
                        .lineLimit(3...8)
                        .accessibilityIdentifier("record-goal")
                } header: {
                    Text("Goal")
                } footer: {
                    Text("The agent reads the goal from .muxa/TASK.md in its directory.")
                }
                Section {
                    Toggle("In a repository", isOn: $withRepo)
                        .accessibilityIdentifier("record-with-repo")
                    if withRepo {
                        repoFields
                    }
                } footer: {
                    Text(withRepo
                         ? "The host creates a worktree on the branch. A new branch starts from the base."
                         : "The host creates a directory for the task in its workspace.")
                }
                NewTaskLoop(draft: $draft)
                if let message = error ?? visibleProblem {
                    Section {
                        Label(message, systemImage: Icon.warning)
                            .foregroundStyle(Theme.failure)
                            .accessibilityIdentifier("record-error")
                    }
                }
            }
            .navigationTitle("New Task")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }
                        .keyboardShortcut(.cancelAction)
                        .disabled(creating)
                }
                ToolbarItem(placement: .confirmationAction) {
                    if creating {
                        ProgressView()
                    } else {
                        Button("Start", action: create)
                            .disabled(request == nil)
                            .accessibilityIdentifier("record-start")
                    }
                }
            }
            .interactiveDismissDisabled(creating)
            .onAppear { titleFocused = true }
            .onChange(of: draft) { error = nil }
            // Another workspace allows other checks.
            .onChange(of: draft.workspace) {
                if draft.loop != nil { draft.loop = model.checks(of: draft.workspace).map { LoopDraft(checks: $0) } }
            }
            .onChange(of: withRepo) { _, on in
                error = nil
                if on, draft.repo.isEmpty { draft.repo = model.session.sessions.knownRepos.first?.dir ?? "" }
            }
        }
    }

    @ViewBuilder
    private var repoFields: some View {
        let repos = model.session.sessions.knownRepos
        if repos.isEmpty {
            repoPath
        } else {
            Picker("Repository", selection: $draft.repo) {
                Text("Other path").tag("")
                ForEach(repos) { repo in
                    Text(repo.name).tag(repo.dir)
                }
            }
            if !repos.contains(where: { $0.dir == draft.repo }) { repoPath }
        }
        TextField("feature/name", text: $draft.branch)
            .textInputAutocapitalization(.never)
            .autocorrectionDisabled()
            .accessibilityIdentifier("record-branch")
        TextField("Base (default branch)", text: $draft.base)
            .textInputAutocapitalization(.never)
            .autocorrectionDisabled()
    }

    private var repoPath: some View {
        TextField("/path/to/repo", text: $draft.repo)
            .textInputAutocapitalization(.never)
            .autocorrectionDisabled()
            .accessibilityIdentifier("record-repo")
    }

    /// The draft as sent: without the repo fields while the toggle is off.
    private var effective: TaskRecordDraft {
        guard !withRepo else { return draft }
        var d = draft
        d.repo = ""
        return d
    }

    private var request: NewTaskRecord? { effective.request }

    /// The draft's problem, once a title has been typed.
    private var visibleProblem: String? {
        draft.title.isEmpty ? nil : effective.problem
    }

    private func create() {
        guard let request, !creating else { return }
        creating = true
        Task {
            defer { creating = false }
            do {
                let record = try await model.session.createTask(request)
                dismiss()
                created(record)
            } catch {
                self.error = PaneSession.describe(error)
            }
        }
    }
}
