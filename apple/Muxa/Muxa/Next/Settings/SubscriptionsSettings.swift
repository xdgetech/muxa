import MuxaV2
import SwiftUI

/// The host's subscriptions (ADR 0091): agent logins muxa names. First the
/// logins it found that are no subscription yet, then the subscriptions with
/// their 5-hour meter. The login itself stays with the agent.
struct SubscriptionsSettings: View {
    @Environment(NextModel.self) private var model
    @State private var editing: SubscriptionDraft?
    @State private var error: String?

    var body: some View {
        SettingsPage {
            let s = model.settings.settings
            if let found = s?.foundLogins, !found.isEmpty { foundSection(found) }
            Section {
                ForEach(s?.subscriptions ?? []) { sub in
                    Button { editing = SubscriptionDraft(sub) } label: { row(sub) }
                        .buttonStyle(.plain)
                        .accessibilityIdentifier("sub.\(sub.name)")
                }
                Button("Add Subscription", systemImage: "plus") { editing = SubscriptionDraft() }
                    .disabled(s == nil || model.settings.readOnly)
                    .accessibilityIdentifier("subs.add")
            } footer: {
                Text("A subscription is an agent's own login, such as a Claude Max or a ChatGPT plan. "
                    + "muxa keeps its name and folder; the login stays with the agent, and muxa never reads it.")
            }
            SaveError(text: error, id: "subs.error")
        }
        .navigationTitle("Subscriptions")
        .inlineTitle()
        .sheet(item: $editing) { SubscriptionEditor(draft: $0) }
        .task { await model.settings.loadPlanUsage() }
    }

    private func row(_ sub: SubscriptionSpec) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack {
                Text(sub.name).font(.body.weight(.semibold)).foregroundStyle(Palette.text)
                Spacer()
                StateTag(text: sub.agent == "codex" ? "Codex" : "Claude Code")
            }
            Text([sub.plan, PlanWindow.line(model.settings.limits(sub.name))].compactMap { $0 }.joined(separator: " · "))
                .font(.caption)
                .foregroundStyle(Palette.textSecondary)
        }
        .contentShape(Rectangle())
        .accessibilityElement(children: .combine)
    }

    /// Onboarding (ADR 0091 §7): what muxa found, each with a suggested name.
    private func foundSection(_ found: [FoundLogin]) -> some View {
        Section {
            ForEach(found, id: \.self) { f in
                HStack {
                    VStack(alignment: .leading, spacing: 2) {
                        Text(Found.name(f)).foregroundStyle(Palette.text)
                        Text(Found.origin(f)).font(.caption).foregroundStyle(Palette.textSecondary)
                    }
                    Spacer()
                    Button("Add") { add([f]) }
                        .buttonStyle(.borderless)
                        .accessibilityIdentifier("subs.found.add.\(Found.name(f))")
                }
            }
            if found.count > 1 {
                Button("Add All \(found.count)") { add(found) }
                    .accessibilityIdentifier("subs.found.addall")
            }
        } header: {
            Text(found.count == 1 ? "muxa found 1 login" : "muxa found \(found.count) logins")
        } footer: {
            Text("Found by the folder names and the Workspaces' env only. A Workspace keeps the same login: "
                + "its env variable becomes the subscription it lists.")
        }
        .disabled(model.settings.readOnly || model.settings.saving)
    }

    private func add(_ found: [FoundLogin]) {
        Task { error = await model.settings.save(found.map(Found.op)) }
    }
}

/// How onboarding names and adds a found login.
enum Found {
    /// `claude`, `codex`, or `claude-<workspace>` for a Workspace's env login.
    static func name(_ f: FoundLogin) -> String {
        let agent = f.agent == "codex" ? "codex" : "claude"
        guard let w = f.workspace else { return agent }
        let slug = w.lowercased().map { $0.isLetter || $0.isNumber ? $0 : "-" }
        return "\(agent)-\(String(slug))"
    }

    static func origin(_ f: FoundLogin) -> String {
        let agent = f.agent == "codex" ? "Codex" : "Claude Code"
        if let w = f.workspace { return "\(agent) · from \(w)'s env" }
        return "\(agent) · \(f.home ?? "") (the default login)"
    }

    static func op(_ f: FoundLogin) -> SettingsOp {
        if let w = f.workspace {
            return .subscriptionAdopt(workspace: w, agent: f.agent, name: name(f))
        }
        return .subscriptionAdd(subscription: SubscriptionSpec(name: name(f), agent: f.agent, home: f.home ?? ""))
    }
}

struct SubscriptionDraft: Identifiable {
    var existing: String?
    var name = ""
    var agent = "claude-code"
    var home = ""
    var plan = ""
    var id: String { existing ?? "+" }

    init() {}

    init(_ s: SubscriptionSpec) {
        existing = s.name
        name = s.name
        agent = s.agent
        home = s.home
        plan = s.plan ?? ""
    }

    /// `~/.claude-<name>` until the user types a folder.
    var folder: String { Lines.optional(home) ?? "~/.\(agent == "codex" ? "codex" : "claude")-\(Lines.trimmed(name))" }

    func spec() -> SubscriptionSpec? {
        let name = Lines.trimmed(name)
        guard BotSpec.isValidName(name) else { return nil }
        return SubscriptionSpec(name: name, agent: agent, home: folder, plan: Lines.optional(plan))
    }
}

private struct SubscriptionEditor: View {
    @Environment(NextModel.self) private var model
    @Environment(NextNavigation.self) private var nav
    @Environment(\.openWindow) private var openWindow
    @Environment(\.dismiss) private var dismiss
    @State private var draft: SubscriptionDraft
    @State private var loginError: String?

    init(draft: SubscriptionDraft) {
        _draft = State(initialValue: draft)
    }

    var body: some View {
        SettingsEditor(
            title: draft.existing ?? "New Subscription", ops: ops,
            remove: draft.existing.map { (.subscriptionRemove(name: $0), "Its login stays in its folder. A Workspace that lists it must drop it first.") }
        ) {
            Section {
                if draft.existing == nil {
                    TextField("Name, e.g. claude-work", text: $draft.name)
                        .plainEntry()
                        .accessibilityIdentifier("sub.name")
                }
                Picker("Agent", selection: $draft.agent) {
                    Text("Claude Code").tag("claude-code")
                    Text("Codex").tag("codex")
                }
                .accessibilityIdentifier("sub.agent")
                TextField(draft.folder, text: $draft.home)
                    .monospaced()
                    .plainEntry()
                    .accessibilityIdentifier("sub.home")
                TextField("Plan, e.g. Max 20x (a label only)", text: $draft.plan)
                    .accessibilityIdentifier("sub.plan")
            } footer: {
                Text("One folder per account: its CLAUDE_CONFIG_DIR or CODEX_HOME. ~/.claude and ~/.codex are the default logins. "
                    + "Lowercase letters, digits, - and _ in the name.")
            }
            if let name = draft.existing {
                limits(name)
                login(name)
                usedBy(name)
            }
        }
    }

    @ViewBuilder
    private func limits(_ name: String) -> some View {
        let usage = model.settings.limits(name)
        Section {
            if let usage, !usage.windows.isEmpty {
                ForEach(usage.windows, id: \.minutes) { PlanMeter(window: $0) }
            } else {
                Text("No data yet. It shows after the first agent on this login answers.")
                    .foregroundStyle(Palette.textSecondary)
            }
        } header: {
            Text("Limits")
        } footer: {
            if let at = usage?.asOf { Text("As of \(at.formatted(date: .omitted, time: .shortened)).") }
        }
    }

    private func login(_ name: String) -> some View {
        Section {
            Button("Log In on the Host", systemImage: "terminal") {
                Task {
                    if let t = await model.startLogin(name) {
                        dismiss()
                        BotTalk.show(t, nav: nav, openWindow: openWindow)
                    } else {
                        loginError = model.requestError ?? "The login terminal did not start."
                    }
                }
            }
            .accessibilityIdentifier("sub.login")
            SaveError(text: loginError, id: "sub.login.error")
        } footer: {
            Text("Opens a terminal on the host that runs the agent with this folder. Log in there; muxa does not see it. "
                + "In a shell on the host: \(Self.command(draft)).")
        }
    }

    private func usedBy(_ name: String) -> some View {
        let s = model.settings.settings
        let workspaces = (s?.workspaces ?? []).filter { $0.subscriptions.contains(name) }.map(\.name)
        let bots = (s?.workspaces ?? []).flatMap { w in w.bots.filter { $0.spec.subscriptions.contains(name) }.map { "\($0.name) in \(w.name)" } }
        let assistant = s?.assistant?.subscriptions.contains(name) == true ? ["Capture assistant"] : []
        let all = workspaces + bots + assistant
        return Section("Used by") {
            Text(all.isEmpty ? "Nothing lists it yet." : all.joined(separator: ", "))
                .foregroundStyle(Palette.textSecondary)
                .accessibilityIdentifier("sub.usedby")
        }
    }

    static func command(_ d: SubscriptionDraft) -> String {
        let default_ = d.folder == (d.agent == "codex" ? "~/.codex" : "~/.claude")
        if d.agent == "codex" { return default_ ? "codex login" : "CODEX_HOME=\(d.folder) codex login" }
        return default_ ? "claude" : "CLAUDE_CONFIG_DIR=\(d.folder) claude"
    }

    private func ops() -> [SettingsOp]? {
        guard let spec = draft.spec() else { return nil }
        return draft.existing == nil ? [.subscriptionAdd(subscription: spec)] : [.subscriptionUpdate(subscription: spec)]
    }
}
