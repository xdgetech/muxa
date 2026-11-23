import MuxaV2
import SwiftUI

/// The Bots of the Workspace's `.muxa/routines.toml` (ADR 0082).
struct WorkspaceBotsForm: View {
    @Environment(NextModel.self) private var model
    let workspace: SettingsWorkspace
    @State private var editing: BotDraft?

    var body: some View {
        SettingsPage { section }
            .sheet(item: $editing) { BotEditor(workspace: workspace, draft: $0) }
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
    var worker = ""
    var dailyItems = ""
    var enabled = true
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
        soul = b.soulText ?? ""
        readSoul = b.soulText
    }

    func spec() -> BotSpec? {
        guard let name = Lines.optional(name) else { return nil }
        return BotSpec(name: name, description: Lines.optional(description), worker: Lines.optional(worker),
                       dailyItems: UInt32(Lines.trimmed(dailyItems)), enabled: enabled)
    }

    /// The description, its Routines and how its items ended.
    static func summary(_ b: SettingsBot, workspace: SettingsWorkspace, inbox: Inbox) -> String {
        let routines = workspace.routines.filter { $0.spec.bot == b.name }.count
        var parts = [b.spec.description ?? "", routines == 1 ? "1 Routine" : "\(routines) Routines"]
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
    let workspace: SettingsWorkspace
    @State private var draft: BotDraft
    @State private var confirmRestore = false
    @State private var restoreError: String?

    init(workspace: SettingsWorkspace, draft: BotDraft) {
        self.workspace = workspace
        _draft = State(initialValue: draft)
    }

    var body: some View {
        SettingsEditor(
            title: draft.existing?.name ?? "New Bot", ops: ops,
            remove: draft.existing.map {
                (.botRemove(workspace: workspace.name, name: $0.name),
                 "Remove its Routines first. Its soul and memory stay in .muxa/bots/.")
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
            Section {
                Picker("Worker", selection: $draft.worker) {
                    Text("The Workspace's agent").tag("")
                    ForEach(model.settings.settings?.workers ?? []) { Text($0.name).tag($0.name) }
                }
                .accessibilityIdentifier("bot.worker")
                TextField("Items a day, 1 to 50 (default 3)", text: $draft.dailyItems)
                    .numberEntry()
                    .accessibilityIdentifier("bot.daily")
            } footer: {
                Text("Past its daily items, the rest go into one \"N more\" item.")
            }
            soul
            if let bot = draft.existing { memory(bot) }
        }
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
