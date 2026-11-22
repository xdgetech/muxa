import MuxaV2
import SwiftUI

/// The Workspace's connections: each with its variable, account, access,
/// secret state, and who used it when.
struct WorkspaceConnectionsForm: View {
    @Environment(NextModel.self) private var model
    let workspace: SettingsWorkspace
    @State private var editing: ConnectionDraft?

    var body: some View {
        SettingsPage {
            Section { EveryAgentNote() }
            list
        }
        .sheet(item: $editing) { ConnectionEditor(workspace: workspace.name, draft: $0) }
    }

    private var list: some View {
        Section("Connections") {
            ForEach(workspace.connections) { c in
                let used = model.settings.usage[workspace.name]?.connections.first { $0.name == c.name }
                Button { editing = ConnectionDraft(c) } label: { row(c, used: used) }
                    .buttonStyle(.plain)
                    .accessibilityElement(children: .combine)
                    .accessibilityIdentifier("connection.\(c.name)")
            }
            Button("Add Connection", systemImage: "plus") { editing = ConnectionDraft() }
                .accessibilityIdentifier("connection.add")
        }
    }

    private func row(_ c: SettingsConnection, used: WorkspaceConnection?) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack(spacing: 8) {
                Text(c.name).font(.body.weight(.semibold)).foregroundStyle(Palette.text)
                Text(c.spec.env).monospaced().font(.caption).foregroundStyle(Palette.textSecondary)
                Spacer()
                if let access = c.spec.access {
                    StateTag(text: access, color: access == "write" ? Theme.attention : Palette.accent)
                }
                StateTag(text: c.secret.rawValue, color: c.secret.color)
            }
            if let account = c.spec.account {
                Text(account).font(.caption).foregroundStyle(Palette.textSecondary)
            }
            UsageLine(usedBy: used?.usedBy, lastUsed: used?.lastUsed)
        }
        .padding(.vertical, 2)
        .contentShape(Rectangle())
    }
}

struct ConnectionDraft: Identifiable {
    var existing: String?
    var name = ""
    var env = ""
    var account = ""
    var access = ""
    var secret: SecretState = .missing
    var id: String { existing ?? "+" }

    init() {}

    init(_ c: SettingsConnection) {
        existing = c.name
        name = c.name
        env = c.spec.env
        account = c.spec.account ?? ""
        access = c.spec.access ?? ""
        secret = c.secret
    }
}

/// Adds or changes a connection. The secret field is write-only and empty
/// after each save; the host says only whether a secret is set.
struct ConnectionEditor: View {
    let workspace: String
    @State private var draft: ConnectionDraft
    @State private var secret = ""

    init(workspace: String, draft: ConnectionDraft) {
        self.workspace = workspace
        _draft = State(initialValue: draft)
    }

    var body: some View {
        SettingsEditor(
            title: draft.existing ?? "New Connection", ops: ops,
            remove: draft.existing.map {
                (.connectionRemove(workspace: workspace, name: $0), "Its secret is deleted from the host's Keychain.")
            },
            onSaved: { secret = "" }
        ) {
            Section {
                if draft.existing == nil {
                    TextField("Name, e.g. github", text: $draft.name)
                        .plainEntry()
                        .accessibilityIdentifier("connection.name")
                }
                TextField("Variable, e.g. GITHUB_TOKEN", text: $draft.env)
                    .monospaced()
                    .plainEntry()
                    .accessibilityIdentifier("connection.env")
                TextField("Account (optional)", text: $draft.account)
                    .plainEntry()
                    .accessibilityIdentifier("connection.account")
                Picker("Access", selection: $draft.access) {
                    Text("Not said").tag("")
                    Text("read").tag("read")
                    Text("write").tag("write")
                }
                .accessibilityIdentifier("connection.access")
            } footer: {
                Text("Account and access are labels for you. The token decides what an agent can do.")
            }
            Section {
                if draft.existing != nil {
                    LabeledContent("Secret") { StateTag(text: draft.secret.rawValue, color: draft.secret.color) }
                }
                SecureField(draft.existing == nil ? "Secret" : "New secret", text: $secret)
                    .accessibilityIdentifier("connection.secret")
            } header: {
                Text("Secret")
            } footer: {
                Text("It goes only to the host's Keychain and never comes back. Leave it empty to keep the one set.")
            }
        }
    }

    private func ops() -> [SettingsOp]? {
        guard let name = Lines.optional(draft.name), let env = Lines.optional(draft.env) else { return nil }
        let spec = ConnectionSpec(name: name, env: env, account: Lines.optional(draft.account),
                                  access: Lines.optional(draft.access))
        let secret = secret.isEmpty ? nil : secret
        return draft.existing == nil
            ? [.connectionAdd(workspace: workspace, connection: spec, secret: secret)]
            : [.connectionUpdate(workspace: workspace, connection: spec, secret: secret)]
    }
}
