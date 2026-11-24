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
    var enabled = true
    /// The Bots it leads (ADR 0085), in the Workspace's order.
    var members: [String] = []
    var handoffs = ""
    /// Shell commands only this Bot may run (ADR 0088).
    var commands: [EditableLine] = []
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
        enabled = b.spec.enabled
        members = b.spec.members
        handoffs = b.spec.handoffsPerDay.map(String.init) ?? ""
        commands = b.spec.commands.map(EditableLine.init)
        soul = b.soulText ?? ""
        readSoul = b.soulText
    }

    func spec() -> BotSpec? {
        guard let name = Lines.optional(name) else { return nil }
        guard commands.allSatisfy({ Self.commandProblem($0.text) == nil }) else { return nil }
        return BotSpec(name: name, description: Lines.optional(description), worker: Lines.optional(worker),
                       enabled: enabled, members: members,
                       handoffsPerDay: members.isEmpty ? nil : UInt32(Lines.trimmed(handoffs)),
                       commands: commands.map { Lines.trimmed($0.text) })
    }

    /// Why a command cannot be saved: the host takes the inside of `Bash(…)`, 500 bytes at most.
    static func commandProblem(_ text: String) -> String? {
        let line = Lines.trimmed(text)
        if line.isEmpty { return "Empty: write the command or remove the line." }
        if line.utf8.count > 500 { return "At most 500 bytes." }
        if line.contains("(") || line.contains(")") { return "No ( or ): write only what goes inside Bash(…)." }
        return nil
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

    init(workspace: SettingsWorkspace, draft: BotDraft) {
        self.workspace = workspace
        _draft = State(initialValue: draft)
    }

    var body: some View {
        SettingsEditor(
            title: draft.existing?.name ?? "New Bot", ops: ops,
            remove: draft.existing.flatMap { bot in
                // A Bot with Routines removes in its own section, which moves them first.
                BotRemovalSection.owned(bot.name, in: model.settings.workspace(workspace.name) ?? workspace).isEmpty
                    ? (.botRemove(workspace: workspace.name, name: bot.name), BotRemovalSection.keepsFiles(bot.name)) : nil
            }
        ) {
            if let bot = draft.existing {
                // A renamed Bot is another file entry: the editor closes and the list shows it.
                BotRenameSection(workspace: workspace, bot: bot) { _ in dismiss() }
            }
            BotSettingsSections(draft: $draft, workspace: workspace)
            if let bot = draft.existing { talk(bot) }
            soul
            if let bot = draft.existing { memory(bot) }
            if let bot = draft.existing,
               !BotRemovalSection.owned(bot.name, in: model.settings.workspace(workspace.name) ?? workspace).isEmpty {
                BotRemovalSection(workspace: workspace, bot: bot) { dismiss() }
            }
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

    private func startTalk(_ bot: String) async {
        guard let t = await model.startTalk(workspace: workspace.name, bot: bot, title: "Talk to \(bot)") else {
            talkError = model.requestError
            return
        }
        dismiss()
        BotTalk.show(t, nav: nav, openWindow: openWindow)
    }

    private var soul: some View {
        BotSoulSection(text: $draft.soul)
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
        .dialog("Restore the earlier memory?", isPresented: $confirmRestore,
                message: "The memory now is kept in its place, so this can be undone.",
                actionSheet: true, actions: [DialogAction("Restore", id: "bot.confirmrestore") {
                    let op = SettingsOp.botMemoryRestore(workspace: workspace.name, name: bot.name, version: 1)
                    Task { restoreError = await model.settings.save([op]) }
                }])
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

/// Who a Bot is and what it may do, as Form sections: its name and line,
/// on/off, daily items, its team and its commands. The Bot editor and the
/// Mac Bot page's Settings tab share them.
struct BotSettingsSections: View {
    @Binding var draft: BotDraft
    let workspace: SettingsWorkspace

    var body: some View {
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
            let access = "It runs with \(workspace.name)'s access: its connections and MCP servers."
            Text(draft.existing == nil ? "Lowercase letters, digits, - and _. \(access)" : access)
        }
        team
        commands
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

    /// ADR 0088: granted here, or by applying a talk's proposal; a run cannot add one.
    private var commands: some View {
        Section {
            EditableLineList(lines: $draft.commands, placeholder: "/path/collect.sh delivery:*",
                             addTitle: "Add a command", id: "bot.commands", problem: { BotDraft.commandProblem($0) })
        } header: {
            Text("Commands it may run")
        } footer: {
            Text("Shell commands only this Bot may run, such as /path/collect.sh delivery:*. "
                + "In its Routines, any other command is refused; it tells you what it needed.")
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
}

/// The Soul's editor section.
struct BotSoulSection: View {
    @Binding var text: String

    var body: some View {
        Section {
            TextEditor(text: $text)
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
}

/// Edits only a Bot's Soul; its settings stay as they are.
struct BotSoulEditor: View {
    let workspace: SettingsWorkspace
    let bot: SettingsBot
    @State private var text: String

    init(workspace: SettingsWorkspace, bot: SettingsBot) {
        self.workspace = workspace
        self.bot = bot
        _text = State(initialValue: bot.soulText ?? "")
    }

    var body: some View {
        SettingsEditor(title: "\(bot.name)'s Soul", canSave: text != (bot.soulText ?? ""), ops: {
            [.botUpdate(workspace: workspace.name, bot: bot.spec, soulText: text)]
        }) {
            BotSoulSection(text: $text)
        }
    }
}

/// Remove for a Bot with Routines: they move to another Bot or to none,
/// then the Bot goes, in one settings-update.
struct BotRemovalSection: View {
    @Environment(NextModel.self) private var model
    let workspace: SettingsWorkspace
    let bot: SettingsBot
    let onRemoved: () -> Void
    /// The Bot its Routines move to; empty is no Bot.
    @State private var moveTo = ""
    @State private var confirm = false
    @State private var error: String?

    static func owned(_ name: String, in workspace: SettingsWorkspace) -> [SettingsRoutine] {
        workspace.routines.filter { $0.spec.bot == name }
    }

    static func keepsFiles(_ name: String) -> String {
        "Its soul and memory stay in .muxa/bots/\(name)/."
    }

    var body: some View {
        let current = model.settings.workspace(workspace.name) ?? workspace
        let owned = Self.owned(bot.name, in: current)
        Section {
            if !owned.isEmpty {
                Picker("Its Routines move to", selection: $moveTo) {
                    Text("No Bot").tag("")
                    ForEach(current.bots.map(\.name).filter { $0 != bot.name }, id: \.self) { Text($0).tag($0) }
                }
                .accessibilityIdentifier("bot.remove.moveto")
                ForEach(owned) { r in
                    Label(r.name, systemImage: "arrow.turn.down.right")
                        .font(.system(.callout, design: .monospaced))
                        .accessibilityIdentifier("bot.remove.routine.\(r.name)")
                }
            }
            Button("Remove", role: .destructive) { confirm = true }
                .disabled(model.settings.readOnly || model.settings.saving)
                .accessibilityIdentifier("editor.remove")
            SaveError(text: error, id: "bot.remove.error")
        } header: {
            Text("Remove")
        } footer: {
            Text(Self.keepsFiles(bot.name))
        }
        .dialog("Remove \(bot.name)?", isPresented: $confirm,
                message: owned.isEmpty ? Self.keepsFiles(bot.name)
                    : "Its Routines move to \(moveTo.isEmpty ? "no Bot" : moveTo). \(Self.keepsFiles(bot.name))",
                actionSheet: true, actions: [DialogAction("Remove \(bot.name)", role: .destructive, id: "editor.confirmremove") {
                    let ops = current.botRemoval(bot.name, movingRoutinesTo: moveTo.isEmpty ? nil : moveTo)
                    Task {
                        error = await model.settings.save(ops)
                        if error == nil { onRemoved() }
                    }
                }])
    }
}

/// A Bot's name with Rename (ADR 0089): one `bot-rename` moves its Routines,
/// teams, folder and Inbox items to the new name.
struct BotRenameSection: View {
    @Environment(NextModel.self) private var model
    let workspace: SettingsWorkspace
    let bot: SettingsBot
    /// After the host renamed it, with the new name.
    let onRenamed: (String) -> Void
    @State private var name: String
    @State private var error: String?

    init(workspace: SettingsWorkspace, bot: SettingsBot, onRenamed: @escaping (String) -> Void) {
        self.workspace = workspace
        self.bot = bot
        self.onRenamed = onRenamed
        _name = State(initialValue: bot.name)
    }

    var body: some View {
        Section {
            HStack {
                TextField("Name", text: $name, prompt: Text(bot.name))
                    .labelsHidden()
                    .monospaced()
                    .plainEntry()
                    .accessibilityIdentifier("bot.rename.name")
                Button("Rename") { Task { await rename() } }
                    .disabled(problem != nil || trimmed == bot.name || model.settings.saving || model.settings.readOnly)
                    .accessibilityIdentifier("bot.rename")
            }
            SaveError(text: error, id: "bot.rename.error")
        } header: {
            Text("Name")
        } footer: {
            Text(problem ?? "Its Routines, team, soul, memory and Inbox items move to the new name.")
                .foregroundStyle(problem == nil ? Palette.textSecondary : Theme.failure)
        }
        // A Bot's name is lowercase; typing capitals or spaces would only leave Rename disabled.
        .onChange(of: name) { _, typed in
            let fixed = typed.lowercased().replacingOccurrences(of: " ", with: "-")
            if fixed != typed { name = fixed }
        }
        .onChange(of: bot.name) { _, renamed in name = renamed }
    }

    private var trimmed: String { Lines.trimmed(name) }

    /// Why the name cannot be taken; nil when it can, or when unchanged.
    private var problem: String? {
        guard trimmed != bot.name else { return nil }
        if !BotSpec.isValidName(trimmed) { return "A name is 1 to 64 lowercase letters, digits, - and _." }
        let current = model.settings.workspace(workspace.name) ?? workspace
        if current.bots.contains(where: { $0.name == trimmed }) { return "\(workspace.name) has a Bot named \(trimmed)." }
        return nil
    }

    private func rename() async {
        let to = trimmed
        error = await model.settings.save([.botRename(workspace: workspace.name, from: bot.name, to: to)])
        if error == nil { onRenamed(to) }
    }
}
