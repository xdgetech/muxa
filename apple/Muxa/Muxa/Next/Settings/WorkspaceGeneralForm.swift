import MuxaV2
import SwiftUI

/// The root folder, the repos found under it and outside it, the Routines
/// gate, the build paths, and removal.
struct WorkspaceGeneralForm: View {
    @Environment(NextModel.self) private var model
    let workspace: SettingsWorkspace
    @State private var root: String
    @State private var newRepo = ""
    @State private var error: String?
    @State private var confirmRemove = false
    @State private var buildPaths: [EditableLine]

    init(workspace: SettingsWorkspace) {
        self.workspace = workspace
        _root = State(initialValue: workspace.root)
        _buildPaths = State(initialValue: workspace.buildPaths.map(EditableLine.init))
    }

    var body: some View {
        SettingsPage { sections }
            .dialog("Remove \(workspace.name)?", isPresented: $confirmRemove,
                    message: "muxa forgets the Workspace and deletes its connections' secrets from the Keychain. "
                        + "Its folder, its repos and .muxa/ stay on the host.",
                    actionSheet: true, actions: [DialogAction("Remove Workspace", role: .destructive, id: "ws.confirmremove") {
                        save(.workspaceRemove(workspace: workspace.name))
                    }])
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
        if model.settings.settings?.build != nil { build }
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

    /// The paths under each Task worktree that go to the build disk (ADR 0102).
    private var build: some View {
        let paths = buildPaths.map { Lines.trimmed($0.text) }.filter { !$0.isEmpty }
        return Section {
            EditableLineList(lines: $buildPaths, placeholder: "target", addTitle: "Add a path", id: "ws.build.paths",
                             problem: Self.buildPathProblem)
            Button("Save") { save(.workspaceUpdate(workspace: workspace.name, buildPaths: paths)) }
                .disabled(paths == workspace.buildPaths || buildPaths.contains { Self.buildPathProblem($0.text) != nil }
                    || model.settings.saving || model.settings.readOnly)
                .accessibilityIdentifier("ws.build.save")
        } header: {
            Text("Build output")
        } footer: {
            Text("Paths in each Task worktree that go to the build disk. Default: target.")
        }
        .onChange(of: workspace.buildPaths) { _, saved in buildPaths = saved.map(EditableLine.init) }
    }

    /// The host's rule, said early: a relative path of plain names.
    static func buildPathProblem(_ text: String) -> String? {
        let t = Lines.trimmed(text)
        let plain = !t.hasPrefix("/") && !t.hasPrefix("~")
            && !t.split(separator: "/").contains { $0 == "." || $0 == ".." }
        return plain ? nil : "A path inside the worktree, like target or apple/MuxaKit/.build."
    }

    private func save(_ op: SettingsOp) {
        Task { error = await model.settings.save([op]) }
    }
}
