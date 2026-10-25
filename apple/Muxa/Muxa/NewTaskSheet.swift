import MuxaSession
import SwiftUI

/// Starts a task on the host: a worktree on a branch and a session with an agent in it.
struct NewTaskSheet: View {
    @Environment(AppModel.self) private var model
    @State var draft: TaskDraft
    @State private var otherRepo = false
    @State private var starting = false
    @State private var error: String?
    @FocusState private var branchFocused: Bool

    var body: some View {
        NavigationStack {
            Form {
                Section("Repository") {
                    if repos.isEmpty || otherRepo {
                        TextField("/path/to/repo", text: $draft.repoDir)
                            .textInputAutocapitalization(.never)
                            .autocorrectionDisabled()
                            .accessibilityIdentifier("task-repo")
                    } else {
                        Picker("Repository", selection: $draft.repoDir) {
                            ForEach(repos) { repo in
                                Text(repo.name).tag(repo.dir)
                            }
                        }
                        .accessibilityIdentifier("task-repo")
                    }
                    if !repos.isEmpty {
                        Toggle("Other path", isOn: $otherRepo)
                    }
                }
                Section {
                    TextField("feature/name", text: $draft.branch)
                        .focused($branchFocused)
                        .textInputAutocapitalization(.never)
                        .autocorrectionDisabled()
                        .onSubmit(start)
                        .accessibilityIdentifier("task-branch")
                    TextField("Base (default branch)", text: $draft.base)
                        .textInputAutocapitalization(.never)
                        .autocorrectionDisabled()
                } header: {
                    Text("Branch")
                } footer: {
                    Text("An existing branch is checked out; a new one starts from the base. The host creates a worktree and a session with an agent in it.")
                }
                if let message = error ?? visibleProblem {
                    Section {
                        Label(message, systemImage: Icon.warning)
                            .foregroundStyle(Theme.failure)
                            .accessibilityIdentifier("task-error")
                    }
                }
            }
            .navigationTitle("New Task")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { model.newTask = nil }
                        .keyboardShortcut(.cancelAction)
                        .disabled(starting)
                }
                ToolbarItem(placement: .confirmationAction) {
                    if starting {
                        ProgressView()
                    } else {
                        Button("Start", action: start)
                            .disabled(draft.problem != nil)
                            .accessibilityIdentifier("task-start")
                    }
                }
            }
            .interactiveDismissDisabled(starting)
            .onAppear {
                otherRepo = !draft.repoDir.isEmpty && !repos.contains { $0.dir == draft.repoDir }
                branchFocused = true
            }
            .onChange(of: draft) { error = nil }
        }
    }

    private var repos: [KnownRepo] { model.session.sessions.knownRepos }

    /// The draft's problem, once there is a branch to complain about.
    private var visibleProblem: String? {
        draft.branch.isEmpty ? nil : draft.problem
    }

    private func start() {
        guard draft.problem == nil, !starting else { return }
        starting = true
        Task {
            defer { starting = false }
            do {
                try await model.start(draft)
            } catch {
                self.error = PaneSession.describe(error)
            }
        }
    }
}
