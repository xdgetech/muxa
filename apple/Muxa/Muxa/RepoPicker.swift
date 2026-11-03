import MuxaSession
import SwiftUI

/// The New Task sheet's Repository row: the workspace's repos, searchable, and a
/// typed path for a repo the host does not list (ADR 0031).
struct RepoFields: View {
    let choices: [RepoChoice]
    @Binding var repo: String
    /// The typed path is shown although the repo may match a listed one.
    @Binding var otherPath: Bool

    var body: some View {
        if choices.isEmpty {
            RepoPathField(repo: $repo)
        } else {
            NavigationLink {
                RepoPicker(choices: choices, selection: repo, otherPath: otherPath) { dir in
                    otherPath = dir == nil
                    repo = dir ?? ""
                }
            } label: {
                LabeledContent("Repository", value: chosen)
            }
            .accessibilityIdentifier("record-repo-choice")
            if showsPath { RepoPathField(repo: $repo) }
        }
    }

    private var showsPath: Bool { otherPath || !choices.contains { $0.dir == repo } }

    private var chosen: String {
        showsPath ? "Other path" : choices.first { $0.dir == repo }?.name ?? repo
    }
}

private struct RepoPathField: View {
    @Binding var repo: String

    var body: some View {
        TextField("/path/to/repo", text: $repo)
            .textInputAutocapitalization(.never)
            .autocorrectionDisabled()
            .accessibilityIdentifier("record-repo")
    }
}

/// The list behind the Repository row. A pick returns at once; nil is Other path.
private struct RepoPicker: View {
    let choices: [RepoChoice]
    let selection: String
    let otherPath: Bool
    let pick: (String?) -> Void
    @Environment(\.dismiss) private var dismiss
    @State private var query = ""

    var body: some View {
        List {
            ForEach(shown) { choice in
                Button {
                    pick(choice.dir)
                    dismiss()
                } label: {
                    row(choice)
                }
                .foregroundStyle(.primary)
                .accessibilityIdentifier("repo-\(choice.name)")
            }
            Button {
                pick(nil)
                dismiss()
            } label: {
                HStack {
                    Text("Other path")
                    Spacer()
                    if otherPath { Image(systemName: "checkmark") }
                }
            }
            .foregroundStyle(.primary)
            .accessibilityIdentifier("repo-other")
        }
        .navigationTitle("Repository")
        .searchable(text: $query, placement: .navigationBarDrawer(displayMode: .always), prompt: "Search repos")
        .textInputAutocapitalization(.never)
        .autocorrectionDisabled()
    }

    private var shown: [RepoChoice] {
        query.isEmpty ? choices : choices.filter { $0.name.localizedCaseInsensitiveContains(query) }
    }

    private func row(_ choice: RepoChoice) -> some View {
        HStack {
            VStack(alignment: .leading) {
                Text(choice.name)
                if choice.name != choice.dir {
                    Text(choice.dir).font(.caption).foregroundStyle(.secondary)
                }
            }
            Spacer()
            if !otherPath, choice.dir == selection { Image(systemName: "checkmark") }
        }
    }
}
