import MuxaV2
import SwiftUI

/// A new Workspace: its name, root folder and repos outside the root.
struct WorkspaceAddSheet: View {
    @State private var name = ""
    @State private var root = ""
    @State private var repos = ""

    var body: some View {
        SettingsEditor(title: "New Workspace", ops: ops) {
            Section {
                TextField("Name", text: $name)
                    .plainEntry()
                    .accessibilityIdentifier("workspace.name")
                TextField("Root folder, e.g. ~/Projects/app", text: $root)
                    .plainEntry()
                    .accessibilityIdentifier("workspace.root")
            } footer: {
                Text("The root folder must exist on the host. Its git repos, one or two levels down, are the Workspace's repos.")
            }
            Section {
                TextField("One path per line", text: $repos, axis: .vertical)
                    .lineLimit(1...6)
                    .plainEntry()
                    .accessibilityIdentifier("workspace.repos")
            } header: {
                Text("More repos")
            } footer: {
                Text("Optional: repos outside the root folder.")
            }
        }
    }

    private func ops() -> [SettingsOp]? {
        guard let name = Lines.optional(name), let root = Lines.optional(root) else { return nil }
        return [.workspaceAdd(name: name, root: root, repos: Lines.split(repos))]
    }
}
