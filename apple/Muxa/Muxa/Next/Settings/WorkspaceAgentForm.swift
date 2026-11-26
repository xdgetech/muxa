import MuxaV2
import SwiftUI

/// How the Workspace's agents start: the command, their environment, and the
/// checks a Task Loop may run.
struct WorkspaceAgentForm: View {
    @Environment(NextModel.self) private var model
    let workspace: SettingsWorkspace
    @State private var command: String
    @State private var env: EnvDraft?
    @State private var error: String?
    @State private var subscriptions: [String]

    init(workspace: SettingsWorkspace) {
        self.workspace = workspace
        _command = State(initialValue: workspace.agent.command ?? "")
        _subscriptions = State(initialValue: workspace.subscriptions)
    }

    var body: some View {
        SettingsPage { sections }
            .sheet(item: $env) { EnvEditor(workspace: workspace.name, draft: $0) }
    }

    @ViewBuilder
    private var sections: some View {
        Section {
            HStack {
                TextField("The daemon's --task-command", text: $command)
                    .monospaced()
                    .plainEntry()
                    .accessibilityIdentifier("agent.command")
                if Lines.optional(command) != workspace.agent.command {
                    Button("Save") {
                        let change: FieldChange<String> = Lines.optional(command).map { .set($0) } ?? .clear
                        save([.workspaceUpdate(workspace: workspace.name, agentCommand: change)])
                    }
                    .accessibilityIdentifier("agent.command.save")
                }
            }
        } header: {
            Text("Agent command")
        } footer: {
            Text("Empty: the daemon's --task-command, usually claude.")
        }
        logins
        environment
        SaveError(text: error, id: "settings.error")
        LoopChecksSection(workspace: workspace)
    }

    private var environment: some View {
        Section {
            ForEach(workspace.agent.env) { e in
                Button { env = EnvDraft(key: e.key, isNew: false) } label: {
                    HStack {
                        Text(e.key).monospaced().foregroundStyle(Palette.text)
                        Spacer()
                        StateTag(text: e.set ? "set" : "empty", color: e.set ? Theme.connected : Palette.textSecondary)
                    }
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .accessibilityIdentifier("env.\(e.key)")
            }
            Button("Add Variable", systemImage: "plus") { env = EnvDraft(key: "", isNew: true) }
                .accessibilityIdentifier("env.add")
        } header: {
            Text("Environment")
        } footer: {
            Text("Every agent of the Workspace gets these. Values are write-only: the host never sends them back.")
        }
    }

    /// The subscriptions its agents use, in order (ADR 0091): each agent takes
    /// the first of its kind. A login its env names is adopted from here.
    private var logins: some View {
        Section {
            SubscriptionOrder(names: $subscriptions, available: model.settings.subscriptionNames(), id: "ws.subs") { name in
                save([.workspaceUpdate(workspace: workspace.name,
                                       subscriptions: SubscriptionOrder.first(name, in: subscriptions))])
            }
            if subscriptions.count > 1 {
                // A closure, not `set: save`: see ClaudeConnectorsToggle.
                Toggle("Switch when full", isOn: Binding(
                    get: { workspace.switchSubscriptions },
                    set: { on in save([.workspaceUpdate(workspace: workspace.name, switchSubscriptions: on)]) }))
                    .disabled(model.settings.readOnly || model.settings.saving)
                    .accessibilityIdentifier("ws.subs.switch")
            }
            if subscriptions != workspace.subscriptions {
                Button("Save Subscriptions") { save([.workspaceUpdate(workspace: workspace.name, subscriptions: subscriptions)]) }
                    .accessibilityIdentifier("ws.subs.save")
            }
            ForEach(envLogins, id: \.self) { f in
                Button("Make Its Env Login a Subscription", systemImage: "arrow.down.doc") { save([Found.op(f)]) }
                    .accessibilityIdentifier("ws.subs.adopt")
            }
        } header: {
            Text("Subscriptions")
        } footer: {
            Text(envLogins.isEmpty ? Self.note(workspace)
                : "Its env names a login. As a subscription, it shows its usage and the Workspace can list a second one.")
        }
        .onChange(of: workspace.subscriptions) { _, new in subscriptions = new }
    }

    static func note(_ ws: SettingsWorkspace) -> String {
        if ws.subscriptions.isEmpty { return "Empty: the agent's default login." }
        return ws.switchSubscriptions
            ? "New agents take the Current login of their kind. At 95% of a limit they take the next one."
            : "New agents take the Current login of their kind, also when it is full. Use another to switch."
    }

    private var envLogins: [FoundLogin] {
        (model.settings.settings?.foundLogins ?? []).filter { $0.workspace == workspace.name }
    }

    private func save(_ ops: [SettingsOp]) {
        Task { error = await model.settings.save(ops) }
    }
}

struct EnvDraft: Identifiable {
    var key: String
    var isNew: Bool
    var id: String { isNew ? "+" : key }
}

/// One variable: a new value replaces the old one, which is never shown.
private struct EnvEditor: View {
    let workspace: String
    let draft: EnvDraft
    @State private var key: String
    @State private var value = ""

    init(workspace: String, draft: EnvDraft) {
        self.workspace = workspace
        self.draft = draft
        _key = State(initialValue: draft.key)
    }

    var body: some View {
        SettingsEditor(
            title: draft.isNew ? "New Variable" : draft.key, ops: ops,
            remove: draft.isNew ? nil : (.envUnset(workspace: workspace, key: draft.key), "Agents started later do not get it."),
            onSaved: { value = "" }
        ) {
            Section {
                if draft.isNew {
                    TextField("Name, e.g. CLAUDE_CONFIG_DIR", text: $key)
                        .monospaced()
                        .plainEntry()
                        .accessibilityIdentifier("env.key")
                }
                SecureField(draft.isNew ? "Value" : "New value", text: $value)
                    .accessibilityIdentifier("env.value")
            } footer: {
                Text(draft.isNew ? "The value goes only to the host." : "The value is set. Type a new one to replace it.")
            }
        }
    }

    private func ops() -> [SettingsOp]? {
        guard let key = Lines.optional(key), !value.isEmpty else { return nil }
        return [.envSet(workspace: workspace, key: key, value: value)]
    }
}

/// Task Loops: off, any command, or a list of commands.
private struct LoopChecksSection: View {
    @Environment(NextModel.self) private var model
    let workspace: SettingsWorkspace
    @State private var mode: Mode
    @State private var checks: [String]
    @State private var newCheck = ""
    @State private var error: String?

    enum Mode: String, CaseIterable, Identifiable {
        case off = "Off", any = "Any command", list = "These commands"
        var id: Self { self }
    }

    init(workspace: SettingsWorkspace) {
        self.workspace = workspace
        switch workspace.loop?.checks {
        case nil:
            _mode = State(initialValue: .off)
            _checks = State(initialValue: [])
        case .any:
            _mode = State(initialValue: .any)
            _checks = State(initialValue: [])
        case .list(let list):
            _mode = State(initialValue: .list)
            _checks = State(initialValue: list)
        }
    }

    var body: some View {
        Section {
            Picker("Task Loop checks", selection: $mode) {
                ForEach(Mode.allCases) { Text($0.rawValue).tag($0) }
            }
            .accessibilityIdentifier("loop.mode")
            if mode == .list { list }
            if wanted != workspace.loop?.checks {
                Button("Save Checks") {
                    Task { error = await model.settings.save([.loopChecks(workspace: workspace.name, checks: wanted)]) }
                }
                .disabled(mode == .list && checks.isEmpty)
                .accessibilityIdentifier("loop.save")
            }
            SaveError(text: error, id: "loop.error")
        } header: {
            Text("Task Loop")
        } footer: {
            Text("A Task Loop runs a check after each finished turn. Any command lets a client run whatever it names.")
        }
    }

    @ViewBuilder
    private var list: some View {
        ForEach(checks, id: \.self) { check in
            HStack {
                Text(check).monospaced().foregroundStyle(Palette.text)
                Spacer()
                Button("Remove \(check)", systemImage: "minus.circle") { checks.removeAll { $0 == check } }
                    .labelStyle(.iconOnly)
                    .buttonStyle(.borderless)
                    .foregroundStyle(Theme.failure)
                    .frame(minWidth: minTouch, minHeight: minTouch)
            }
        }
        HStack {
            TextField("Add a check, e.g. make test", text: $newCheck)
                .monospaced()
                .plainEntry()
                .accessibilityIdentifier("loop.check.new")
            Button("Add") {
                let c = Lines.trimmed(newCheck)
                if !checks.contains(c) { checks.append(c) }
                newCheck = ""
            }
            .disabled(Lines.trimmed(newCheck).isEmpty)
            .accessibilityIdentifier("loop.check.add")
        }
    }

    private var wanted: LoopChecks? {
        switch mode {
        case .off: nil
        case .any: .any
        case .list: .list(checks)
        }
    }
}
