import MuxaV2
import SwiftUI

/// The root folder, the repos found under it and outside it, the Routines
/// gate, and removal.
struct WorkspaceGeneralForm: View {
    @Environment(NextModel.self) private var model
    let workspace: SettingsWorkspace
    @State private var root: String
    @State private var newRepo = ""
    @State private var error: String?
    @State private var confirmRemove = false

    init(workspace: SettingsWorkspace) {
        self.workspace = workspace
        _root = State(initialValue: workspace.root)
    }

    var body: some View {
        SettingsPage { sections }
            .confirmationDialog("Remove \(workspace.name)?", isPresented: $confirmRemove, titleVisibility: .visible) {
                Button("Remove Workspace", role: .destructive) { save(.workspaceRemove(workspace: workspace.name)) }
                    .accessibilityIdentifier("ws.confirmremove")
            } message: {
                Text("muxa forgets the Workspace and deletes its connections' secrets from the Keychain. "
                    + "Its folder, its repos and .muxa/ stay on the host.")
            }
    }

    @ViewBuilder
    private var sections: some View {
        Section {
            HStack {
                TextField("Root folder", text: $root)
                    .plainEntry()
                    .accessibilityIdentifier("ws.root")
                if Lines.trimmed(root) != workspace.root {
                    Button("Save") { save(.workspaceUpdate(workspace: workspace.name, root: Lines.trimmed(root))) }
                        .disabled(Lines.trimmed(root).isEmpty)
                        .accessibilityIdentifier("ws.root.save")
                }
            }
        } header: {
            Text("Root folder")
        } footer: {
            Text("It must exist on the host. ~/ is the home folder.")
        }
        found
        repos
        routines
        SaveError(text: error, id: "settings.error")
        Section {
            Button("Remove Workspace", role: .destructive) { confirmRemove = true }
                .accessibilityIdentifier("ws.remove")
        }
    }

    /// The repos the host finds under the root (ADR 0031), each shown or hidden.
    @ViewBuilder
    private var found: some View {
        let found = workspace.foundRepos.filter { !workspace.repos.contains($0) }
        if !found.isEmpty {
            Section {
                ForEach(found, id: \.self) { repo in
                    Toggle(isOn: shown(repo)) {
                        Text(repo).monospaced()
                            .foregroundStyle(workspace.hiddenRepos.contains(repo) ? Palette.textTertiary : Palette.text)
                    }
                    .accessibilityIdentifier("ws.found.\(repo)")
                }
            } header: {
                Text("Found under the root")
            } footer: {
                Text("Git repos one or two folders under the root belong to this Workspace on their own. "
                    + "Hide the ones you do not use: Focus, Start Task and Tasks that read all repos leave them out.")
            }
        }
    }

    private func shown(_ repo: String) -> Binding<Bool> {
        Binding(
            get: { !workspace.hiddenRepos.contains(repo) },
            set: { show in
                let hidden = show ? workspace.hiddenRepos.filter { $0 != repo } : workspace.hiddenRepos + [repo]
                save(.workspaceUpdate(workspace: workspace.name, hiddenRepos: hidden))
            })
    }

    private var repos: some View {
        Section {
            ForEach(workspace.repos, id: \.self) { repo in
                HStack {
                    Text(repo).monospaced().foregroundStyle(Palette.text)
                    Spacer()
                    Button("Remove \(repo)", systemImage: "minus.circle") {
                        save(.workspaceUpdate(workspace: workspace.name, repos: workspace.repos.filter { $0 != repo }))
                    }
                    .labelStyle(.iconOnly)
                    .buttonStyle(.borderless)
                    .foregroundStyle(Theme.failure)
                    .frame(minWidth: minTouch, minHeight: minTouch)
                }
            }
            HStack {
                TextField("Add a repo path", text: $newRepo)
                    .plainEntry()
                    .accessibilityIdentifier("ws.repo.new")
                Button("Add") {
                    save(.workspaceUpdate(workspace: workspace.name, repos: workspace.repos + [Lines.trimmed(newRepo)]))
                    newRepo = ""
                }
                .disabled(Lines.trimmed(newRepo).isEmpty)
                .accessibilityIdentifier("ws.repo.add")
            }
        } header: {
            Text("More repos")
        } footer: {
            Text("Repos outside the root folder. The git repos under the root are found on their own.")
        }
    }

    private var routines: some View {
        Section {
            Toggle("Routines allowed", isOn: Binding(
                get: { workspace.routinesEnabled },
                set: { save(.routinesGate(workspace: workspace.name, enabled: $0, directWrites: workspace.directWrites)) }))
                .accessibilityIdentifier("ws.routines")
            Toggle("Direct writes", isOn: Binding(
                get: { workspace.directWrites },
                set: { save(.routinesGate(workspace: workspace.name, enabled: workspace.routinesEnabled, directWrites: $0)) }))
                .disabled(!workspace.routinesEnabled)
                .accessibilityIdentifier("ws.directwrites")
        } header: {
            Text("Routines")
        } footer: {
            Text("Direct writes let a Routine change outside systems without a proposal, when the Routine asks for it.")
        }
    }

    private func save(_ op: SettingsOp) {
        Task { error = await model.settings.save([op]) }
    }
}
