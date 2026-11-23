import MuxaV2
import SwiftUI

/// The Bots of the Workspace's `.muxa/routines.toml` (ADR 0082).
struct WorkspaceBotsForm: View {
    @Environment(NextModel.self) private var model
    let workspace: SettingsWorkspace
    @State private var editing: BotDraft?
    @State private var talking = false

    var body: some View {
        SettingsPage { section }
            .sheet(item: $editing) { BotEditor(workspace: workspace, draft: $0) }
            .newBotTalk(workspace: workspace.name, isPresented: $talking)
    }

    private var section: some View {
        Section {
            ForEach(workspace.bots) { b in
                Button { editing = BotDraft(b) } label: {
                    VStack(alignment: .leading, spacing: 3) {
                        HStack {
                            Text(b.name).font(.body.weight(.semibold)).foregroundStyle(Palette.text)
                            Spacer()
                            if !b.spec.enabled { StateTag(text: "off") }
                        }
                        Text(BotDraft.summary(b, workspace: workspace, inbox: model.inbox))
                            .font(.caption)
                            .foregroundStyle(Palette.textSecondary)
                    }
                    .padding(.vertical, 2)
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .accessibilityElement(children: .combine)
                .accessibilityIdentifier("settings.bot.\(b.name)")
            }
            Button("Add Bot", systemImage: "plus") { editing = BotDraft() }
                .accessibilityIdentifier("bot.add")
            Button("Set up a Bot by Talking", systemImage: "bubble.left.and.bubble.right") { talking = true }
                .accessibilityIdentifier("bot.talknew")
        } header: {
            Text("Bots")
        } footer: {
            Text("A Bot owns Routines. Each run starts from its soul and memory, and puts at most its daily items in the Inbox.")
        }
    }
}

struct BotDraft: Identifiable {
    var existing: SettingsBot?
    var name = ""
    var description = ""
    /// Kept as the file says it; the host ignores a Bot's worker (ADR 0087).
    var worker = ""
    var dailyItems = ""
    var enabled = true
    /// The Bots it leads (ADR 0085), in the Workspace's order.
    var members: [String] = []
    var handoffs = ""
    var soul = ""
    /// The Soul as read; nil when it could not be read.
    var readSoul: String?
    var id: String { existing?.name ?? "+" }

    init() {}

    init(_ b: SettingsBot) {
        existing = b
        name = b.name
        description = b.spec.description ?? ""
        worker = b.spec.worker ?? ""
        dailyItems = b.spec.dailyItems.map(String.init) ?? ""
        enabled = b.spec.enabled
        members = b.spec.members
        handoffs = b.spec.handoffsPerDay.map(String.init) ?? ""
        soul = b.soulText ?? ""
        readSoul = b.soulText
    }

    func spec() -> BotSpec? {
        guard let name = Lines.optional(name) else { return nil }
        return BotSpec(name: name, description: Lines.optional(description), worker: Lines.optional(worker),
                       dailyItems: UInt32(Lines.trimmed(dailyItems)), enabled: enabled, members: members,
                       handoffsPerDay: members.isEmpty ? nil : UInt32(Lines.trimmed(handoffs)))
    }

    /// The other Bots it may lead: those that lead no team themselves.
    func candidates(in workspace: SettingsWorkspace) -> [String] {
        workspace.bots.filter { $0.name != (existing?.name ?? name) && $0.spec.members.isEmpty }.map(\.name)
    }

    /// The Bot that leads this one; a member cannot lead a team itself.
    func lead(in workspace: SettingsWorkspace) -> String? {
        let me = existing?.name ?? name
        return workspace.bots.first { $0.name != me && $0.spec.members.contains(me) }?.name
    }

    /// The description, its Routines and how its items ended.
    static func summary(_ b: SettingsBot, workspace: SettingsWorkspace, inbox: Inbox) -> String {
        let routines = workspace.routines.filter { $0.spec.bot == b.name }.count
        var parts = [b.spec.description ?? "", routines == 1 ? "1 Routine" : "\(routines) Routines"]
        if !b.spec.members.isEmpty { parts.append("Leads \(b.spec.members.joined(separator: ", "))") }
        if let count = inbox.bots?.first(where: { $0.workspace == workspace.name && $0.bot == b.name }) {
            parts.append("\(count.done) done · \(count.deleted) deleted")
        }
        return parts.filter { !$0.isEmpty }.joined(separator: " · ")
    }
}

/// Adds or changes a Bot: who it is, its daily limit, and its Soul. Its
/// Memory is the Bot's own; only an earlier one can be brought back.
struct BotEditor: View {
    @Environment(NextModel.self) private var model
    @Environment(NextNavigation.self) private var nav
    @Environment(\.dismiss) private var dismiss
    @Environment(\.openWindow) private var openWindow
    let workspace: SettingsWorkspace
    @State private var draft: BotDraft
    @State private var confirmRestore = false
    @State private var restoreError: String?
    @State private var talkError: String?
    /// Remove with Routines: the Bot they move to; empty is no Bot.
    @State private var moveTo = ""
    @State private var confirmRemove = false
    @State private var removeError: String?

    init(workspace: SettingsWorkspace, draft: BotDraft) {
        self.workspace = workspace
        _draft = State(initialValue: draft)
    }

    var body: some View {
        SettingsEditor(
            title: draft.existing?.name ?? "New Bot", ops: ops,
            remove: draft.existing.flatMap { bot in
                // A Bot with Routines removes in its own section, which moves them first.
                owned(bot).isEmpty
                    ? (.botRemove(workspace: workspace.name, name: bot.name), keepsFiles(bot.name)) : nil
            }
        ) {
            Section {
                if draft.existing == nil {
                    TextField("Name, e.g. pr-watch", text: $draft.name)
                        .plainEntry()
                        .accessibilityIdentifier("bot.name")
                }
                TextField("What it does, in one line", text: $draft.description)
                    .accessibilityIdentifier("bot.description")
                Toggle("Enabled", isOn: $draft.enabled)
                    .accessibilityIdentifier("bot.enabled")
            } footer: {
                if draft.existing == nil { Text("Lowercase letters, digits, - and _.") }
            }
            if let bot = draft.existing { talk(bot) }
            Section {
                TextField("Items a day, 1 to 50 (default 3)", text: $draft.dailyItems)
                    .numberEntry()
                    .accessibilityIdentifier("bot.daily")
            } footer: {
                Text("Past its daily items, the rest go into one \"N more\" item. It runs with \(workspace.name)'s access: its connections and MCP servers.")
            }
            team
            soul
            if let bot = draft.existing { memory(bot) }
            if let bot = draft.existing, !owned(bot).isEmpty { removal(bot) }
        }
    }

    /// Its Routines in the settings as they are now.
    private func owned(_ bot: SettingsBot) -> [SettingsRoutine] {
        (model.settings.workspace(workspace.name) ?? workspace).routines.filter { $0.spec.bot == bot.name }
    }

    private func keepsFiles(_ name: String) -> String {
        "Its soul and memory stay in .muxa/bots/\(name)/."
    }

    /// Remove for a Bot with Routines: they move to another Bot or to none,
    /// then the Bot goes, in one settings-update.
    private func removal(_ bot: SettingsBot) -> some View {
        let current = model.settings.workspace(workspace.name) ?? workspace
        let others = current.bots.map(\.name).filter { $0 != bot.name }
        return Section {
            Picker("Its Routines move to", selection: $moveTo) {
                Text("No Bot").tag("")
                ForEach(others, id: \.self) { Text($0).tag($0) }
            }
            .accessibilityIdentifier("bot.remove.moveto")
            ForEach(owned(bot)) { r in
                Label(r.name, systemImage: "arrow.turn.down.right")
                    .font(.system(.callout, design: .monospaced))
                    .accessibilityIdentifier("bot.remove.routine.\(r.name)")
            }
            Button("Remove", role: .destructive) { confirmRemove = true }
                .disabled(model.settings.readOnly || model.settings.saving)
                .accessibilityIdentifier("editor.remove")
            SaveError(text: removeError, id: "bot.remove.error")
        } header: {
            Text("Remove")
        } footer: {
            Text(keepsFiles(bot.name))
        }
        .confirmationDialog("Remove \(bot.name)?", isPresented: $confirmRemove, titleVisibility: .visible) {
            Button("Remove \(bot.name)", role: .destructive) {
                let ops = current.botRemoval(bot.name, movingRoutinesTo: moveTo.isEmpty ? nil : moveTo)
                Task {
                    removeError = await model.settings.save(ops)
                    if removeError == nil { dismiss() }
                }
            }
            .accessibilityIdentifier("editor.confirmremove")
        } message: {
            Text("Its Routines move to \(moveTo.isEmpty ? "no Bot" : moveTo). \(keepsFiles(bot.name))")
        }
    }

    /// Talks with the Bot in a Task: it can propose its settings (ADR 0083).
    private func talk(_ bot: SettingsBot) -> some View {
        Section {
            Button("Talk to \(bot.name)", systemImage: "bubble.left.and.bubble.right") { Task { await startTalk(bot.name) } }
                .accessibilityIdentifier("bot.talk")
            SaveError(text: talkError, id: "bot.talk.error")
        } footer: {
            Text("A Task with the Bot's soul and memory. What it proposes comes to the Inbox to apply.")
        }
    }

    /// A team lead hands work to its members (ADR 0085). A member leads no team.
    @ViewBuilder
    private var team: some View {
        let candidates = draft.candidates(in: workspace)
        if draft.lead(in: workspace) == nil, !candidates.isEmpty {
            Section {
                ForEach(candidates, id: \.self) { name in
                    Toggle(name, isOn: member(name))
                        .accessibilityIdentifier("bot.member.\(name)")
                }
                if !draft.members.isEmpty {
                    TextField("Handoffs a day, 1 to 50 (default 10)", text: $draft.handoffs)
                        .numberEntry()
                        .accessibilityIdentifier("bot.handoffs")
                }
            } header: {
                Text("Team")
            } footer: {
                Text("Members are Bots that lead no team. The lead hands them work, up to its handoffs a day.")
            }
        }
    }

    private func member(_ name: String) -> Binding<Bool> {
        Binding(
            get: { draft.members.contains(name) },
            set: { on in
                let picked = Set(draft.members.filter { $0 != name } + (on ? [name] : []))
                draft.members = workspace.bots.map(\.name).filter(picked.contains)
            })
    }

    private func startTalk(_ bot: String) async {
        guard let t = await model.startTalk(workspace: workspace.name, bot: bot, title: "Talk to \(bot)") else {
            talkError = model.requestError
            return
        }
        dismiss()
        BotTalk.show(t, nav: nav, openWindow: openWindow)
    }

    private var soul: some View {
        Section {
            TextEditor(text: $draft.soul)
                .font(.system(.callout, design: .monospaced))
                .frame(minHeight: 160)
                .scrollContentBackground(.hidden)
                .accessibilityIdentifier("bot.soul")
        } header: {
            Text("Soul")
        } footer: {
            Text("Who the Bot is, what it watches and what it must not do. Every run starts from it. At most 12 KiB.")
        }
    }

    private func memory(_ bot: SettingsBot) -> some View {
        Section {
            Text(bot.memoryText.map { $0.isEmpty ? "Nothing yet." : $0 } ?? "The memory cannot be read.")
                .font(.system(.callout, design: .monospaced))
                .foregroundStyle(Palette.textSecondary)
                .textSelection(.enabled)
                .accessibilityIdentifier("bot.memory")
            if bot.memoryVersions > 0 {
                Button("Restore Earlier Memory") { confirmRestore = true }
                    .accessibilityIdentifier("bot.restore")
            }
            SaveError(text: restoreError, id: "bot.restore.error")
        } header: {
            Text("Memory")
        } footer: {
            Text("The Bot writes this after each run. The five before it are kept.")
        }
        .confirmationDialog("Restore the earlier memory?", isPresented: $confirmRestore, titleVisibility: .visible) {
            Button("Restore") {
                let op = SettingsOp.botMemoryRestore(workspace: workspace.name, name: bot.name, version: 1)
                Task { restoreError = await model.settings.save([op]) }
            }
            .accessibilityIdentifier("bot.confirmrestore")
        } message: {
            Text("The memory now is kept in its place, so this can be undone.")
        }
    }

    private func ops() -> [SettingsOp]? {
        guard let spec = draft.spec() else { return nil }
        // An unchanged Soul is not sent, so a file the host could not read stays as it is.
        let text = draft.existing != nil && draft.soul == (draft.readSoul ?? "") ? nil : draft.soul
        return draft.existing == nil
            ? [.botAdd(workspace: workspace.name, bot: spec, soulText: text)]
            : [.botUpdate(workspace: workspace.name, bot: spec, soulText: text)]
    }
}
