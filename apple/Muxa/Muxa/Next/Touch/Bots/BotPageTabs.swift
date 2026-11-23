import MuxaV2
import SwiftUI

/// A Bot's Home: its tiles, then Needs you, In progress, Planned and Recent
/// (ADR 0086). An item opens on this page; a terminal opens its conversation.
struct BotHomeList: View {
    @Environment(NextModel.self) private var model
    @Environment(NextNavigation.self) private var nav
    let entry: BotEntry
    let showChats: () -> Void

    private var work: BotWork { entry.work }

    var body: some View {
        List {
            if !tiles.isEmpty {
                LazyVGrid(columns: [GridItem(.flexible(), spacing: 8), GridItem(.flexible(), spacing: 8)], spacing: 8) {
                    ForEach(tiles, id: \.0) { BotTile(label: $0.0, value: $0.1) }
                }
                .cardRow(vertical: 6)
            }
            if model.routines(of: entry).isEmpty {
                VStack(alignment: .leading, spacing: 8) {
                    Text("No Routines yet").font(.headline).foregroundStyle(Palette.text)
                    Text("Tell \(entry.name) in a chat what to watch. It proposes Routines you apply.")
                        .foregroundStyle(Palette.textSecondary)
                    Button("Open chat") { Task { await model.newChat(with: entry, nav: nav) } }
                        .buttonStyle(FilledButtonStyle())
                        .accessibilityIdentifier("bot.home.openchat")
                }
                .padding(14)
                .frame(maxWidth: .infinity, alignment: .leading)
                .card()
                .cardRow()
            }
            if !work.needsYou.isEmpty {
                Section {
                    ForEach(work.needsYou) { e in
                        switch e {
                        case let .item(i): itemRow(i)
                        case let .terminal(t): terminalRow(t)
                        }
                    }
                } header: { GroupHeader(title: "Needs you", color: TaskState.needsYou.color, count: work.needsYou.count) }
            }
            if !work.inProgress.isEmpty {
                Section {
                    ForEach(work.inProgress) { terminalRow($0) }
                } header: { GroupHeader(title: "In progress", color: TaskState.working.color, count: work.inProgress.count) }
            }
            if !work.planned.isEmpty {
                Section {
                    ForEach(work.planned) { r in
                        BotListRow(title: r.name, subtitle: r.nextDue.map { "next \(routineStamp($0)) · \(r.when)" } ?? "On review")
                            .cardRow()
                    }
                } header: { GroupHeader(title: "Planned") }
            }
            if !work.recent.isEmpty {
                Section {
                    ForEach(work.recent.prefix(10)) { itemRow($0, done: true) }
                } header: { GroupHeader(title: "Recent", count: work.recent.count) }
            }
        }
        .listStyle(.plain)
        .scrollContentBackground(.hidden)
        .accessibilityIdentifier("bot.home")
    }

    /// Next run, last run and when the memory changed; a tile without data is left out.
    private var tiles: [(String, String)] {
        var out: [(String, String)] = []
        if let next = work.planned.first {
            out.append(("Next run", next.nextDue.map { "\(next.name) · \(routineStamp($0))" } ?? "\(next.name) · On review"))
        }
        if let last = model.routines(of: entry).compactMap({ r in r.last.map { (r, $0) } }).max(by: { $0.1.at < $1.1.at }) {
            out.append(("Last run", "\(routineStamp(last.1.at)) · \(last.1.ok ? "OK" : "Failed")"))
        }
        if let at = entry.bot.memoryHistory.first?.at { out.append(("Memory", "updated \(routineStamp(at))")) }
        return out
    }

    private func itemRow(_ item: InboxItem, done: Bool = false) -> some View {
        NavigationLink(value: BotsDestination.item(id: item.id)) {
            HStack(spacing: 10) {
                if done { StateTag(text: "Done") } else { KindChip(kind: item.kind) }
                BotListRow(title: item.title, subtitle: done ? nil : item.origin)
            }
        }
        .accessibilityIdentifier("bot.item.\(item.id)")
        .cardRow()
    }

    private func terminalRow(_ t: MuxaV2.Terminal) -> some View {
        NavigationLink(value: BotsDestination.conversation(terminal: t.id)) {
            HStack(spacing: 10) {
                StateDot(color: t.state.color)
                BotListRow(title: t.title, subtitle: t.botRole(of: entry.name))
            }
        }
        .accessibilityIdentifier("bot.terminal.\(t.id)")
        .cardRow()
    }
}

/// A title over a subtitle, as a Bot's lists show rows.
struct BotListRow: View {
    let title: String
    var subtitle: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(title).font(.system(size: 15)).foregroundStyle(Palette.text).lineLimit(2)
            if let subtitle {
                Text(subtitle).font(.system(size: 12.5)).foregroundStyle(Palette.textSecondary).lineLimit(1)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.vertical, 4)
    }
}

/// A label over a value, as the Home's tiles.
struct BotTile: View {
    let label: String
    let value: String

    var body: some View {
        VStack(alignment: .leading, spacing: 3) {
            Text(label).font(.system(size: 12)).foregroundStyle(Palette.textTertiary)
            Text(value).font(.system(size: 14)).foregroundStyle(Palette.text).lineLimit(2)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(12)
        .card(radius: 12)
        .accessibilityElement(children: .combine)
    }
}

/// A Bot's chats and runs: New chat first, then each live one; a pick opens
/// its conversation on this page.
struct BotChatsList: View {
    @Environment(NextModel.self) private var model
    @Environment(NextNavigation.self) private var nav
    let entry: BotEntry

    var body: some View {
        let talks = model.talks(of: entry)
        let runs = entry.work.terminals.filter { !talks.contains($0) }
        List {
            Button("New chat", systemImage: "plus") { Task { await model.newChat(with: entry, nav: nav) } }
                .buttonStyle(FilledButtonStyle())
                .disabled(!model.isConnected)
                .accessibilityIdentifier("bot.chats.new")
                .cardRow(vertical: 8)
            Section {
                if talks.isEmpty {
                    Text("No open chats.").foregroundStyle(Palette.textSecondary).cardRow()
                }
                ForEach(talks) { row($0) }
            } header: { GroupHeader(title: "Chats", count: talks.count) }
            if !runs.isEmpty {
                Section {
                    ForEach(runs) { row($0) }
                } header: { GroupHeader(title: "Runs", count: runs.count) }
            }
            Text("Each chat is a Task with the Bot's soul and memory. Close it when the job is done; the next chat starts from what the Bot remembers.")
                .font(.caption)
                .foregroundStyle(Palette.textTertiary)
                .cardRow(vertical: 8, horizontal: 18)
        }
        .listStyle(.plain)
        .scrollContentBackground(.hidden)
    }

    private func row(_ t: MuxaV2.Terminal) -> some View {
        let waiting = model.inbox.items.contains { $0.terminal == t.id && $0.settingsProposal != nil }
        return NavigationLink(value: BotsDestination.conversation(terminal: t.id)) {
            HStack(spacing: 10) {
                StateDot(color: waiting ? Palette.accent : t.state.color)
                BotListRow(title: t.title, subtitle: waiting ? "proposal waiting" : t.botRole(of: entry.name))
                if t.isTaskAgent { Text("Continue").font(.caption).foregroundStyle(Palette.accent) }
            }
        }
        .accessibilityIdentifier("bot.chat.\(t.id)")
        .cardRow()
    }
}

/// A Bot's Routines: on/off, Run now, the last run, Edit and Add as sheets,
/// and asking in a chat for a new one.
struct BotRoutinesList: View {
    @Environment(NextModel.self) private var model
    @Environment(NextNavigation.self) private var nav
    let entry: BotEntry
    @State private var editing: RoutineDraft?

    var body: some View {
        let routines = model.routines(of: entry)
        List {
            if routines.isEmpty {
                Text("No Routines yet. A chat with \(entry.name) can propose them.")
                    .foregroundStyle(Palette.textSecondary)
                    .cardRow(vertical: 8, horizontal: 18)
            }
            ForEach(routines) { row($0) }
            HStack(spacing: 10) {
                Button("Ask in a chat") { Task { await model.newChat(with: entry, nav: nav) } }
                    .buttonStyle(FilledButtonStyle())
                    .disabled(!model.isConnected)
                    .accessibilityIdentifier("bot.routines.ask")
                Button("Add Routine") {
                    var draft = RoutineDraft()
                    draft.bot = entry.name
                    editing = draft
                }
                .buttonStyle(QuietButtonStyle())
                .disabled(model.settings.readOnly || model.settings.settings == nil)
                .accessibilityIdentifier("bot.routines.add")
            }
            .cardRow(vertical: 10)
        }
        .listStyle(.plain)
        .scrollContentBackground(.hidden)
        .sheet(item: $editing) { RoutineEditor(workspace: entry.workspace.name, draft: $0) }
    }

    private func row(_ r: Routine) -> some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack(alignment: .top) {
                VStack(alignment: .leading, spacing: 3) {
                    HStack(spacing: 6) {
                        Text(r.name).font(.system(size: 16, weight: .semibold)).foregroundStyle(Palette.text)
                        if let title = r.health.title { StateTag(text: title, color: r.health.color) }
                    }
                    Text([r.when, lastRun(r)].joined(separator: " · "))
                        .font(.system(size: 12.5))
                        .foregroundStyle(Palette.textSecondary)
                }
                .accessibilityElement(children: .combine)
                .accessibilityIdentifier("bot.routine.\(r.name)")
                Spacer()
                Toggle("On", isOn: Binding(get: { r.enabled }, set: { on in Task { await model.setRoutine(r, enabled: on) } }))
                    .labelsHidden()
                    .disabled(model.settings.readOnly || model.routineSettings(r) == nil || model.settings.saving)
                    .accessibilityIdentifier("bot.routine.on.\(r.name)")
            }
            HStack(spacing: 10) {
                Button("Run now") { Task { await model.runRoutine(r) } }
                    .disabled(!r.enabled || !entry.bot.spec.enabled || r.runningSince != nil)
                    .accessibilityIdentifier("bot.routine.run.\(r.name)")
                Button("Edit") { if let spec = model.routineSettings(r) { editing = RoutineDraft(spec) } }
                    .disabled(model.settings.readOnly || model.routineSettings(r) == nil)
                    .accessibilityIdentifier("bot.routine.edit.\(r.name)")
            }
            .buttonStyle(QuietButtonStyle())
        }
        .padding(14)
        .card()
        .cardRow()
    }

    private func lastRun(_ r: Routine) -> String {
        guard let last = r.last else { return "never ran" }
        return "last \(routineStamp(last.at)) \(last.ok ? "OK" : "failed")"
    }
}

/// A Bot's files: its Soul, which the user writes, and its Memory, which its
/// runs write, with the earlier ones to bring back.
struct BotFilesList: View {
    @Environment(NextModel.self) private var model
    let entry: BotEntry
    @State private var editingSoul = false
    @State private var restoring: MemoryVersion?
    @State private var error: String?

    var body: some View {
        PaletteForm {
            Section {
                Text(entry.bot.soulText.map { $0.isEmpty ? "No soul yet." : $0 } ?? "It cannot be read.")
                    .font(.system(.callout, design: .monospaced))
                    .textSelection(.enabled)
                    .accessibilityIdentifier("bot.files.soul")
                Button("Edit Soul") { editingSoul = true }
                    .disabled(model.settings.readOnly)
                    .accessibilityIdentifier("bot.files.editsoul")
            } header: {
                Text("Soul · who it is · yours to write")
            }
            Section {
                Text(entry.bot.memoryText.map { $0.isEmpty ? "Nothing yet." : $0 } ?? "It cannot be read.")
                    .font(.system(.callout, design: .monospaced))
                    .foregroundStyle(Palette.textSecondary)
                    .textSelection(.enabled)
                    .accessibilityIdentifier("bot.files.memory")
                ForEach(entry.bot.memoryHistory) { v in
                    HStack {
                        Text("Version \(v.version) · replaced \(routineStamp(v.at))").font(.callout)
                        Spacer()
                        Button("Restore") { restoring = v }
                            .disabled(model.settings.readOnly || model.settings.saving)
                            .accessibilityIdentifier("bot.files.restore.\(v.version)")
                    }
                }
                SaveError(text: error, id: "bot.files.error")
            } header: {
                Text("Memory · what it learned")
            } footer: {
                Text(entry.bot.memoryHistory.first.map { "Updated \(routineStamp($0.at)). The five before it are kept." }
                     ?? "The Bot writes this after each run. The five before it are kept.")
            }
        }
        .sheet(isPresented: $editingSoul) { BotSoulEditor(workspace: entry.workspace, bot: entry.bot) }
        .confirmationDialog("Restore memory version \(restoring?.version ?? 0)?",
                            isPresented: Binding(get: { restoring != nil }, set: { if !$0 { restoring = nil } }),
                            titleVisibility: .visible) {
            Button("Restore") {
                guard let v = restoring else { return }
                let op = SettingsOp.botMemoryRestore(workspace: entry.workspace.name, name: entry.name, version: v.version)
                Task { error = await model.settings.save([op]) }
            }
            .accessibilityIdentifier("bot.files.confirmrestore")
        } message: {
            Text("The memory now is kept in its place, so this can be undone.")
        }
    }
}

/// A Bot's settings inline: its name, the shared sections, Save, what it may
/// use, and Remove (ADR 0086–0089).
struct BotSettingsForm: View {
    @Environment(NextModel.self) private var model
    @Environment(NextNavigation.self) private var nav
    @Environment(\.dismiss) private var dismiss
    let entry: BotEntry
    @State private var draft: BotDraft
    @State private var error: String?

    init(entry: BotEntry) {
        self.entry = entry
        _draft = State(initialValue: BotDraft(entry.bot))
    }

    var body: some View {
        PaletteForm {
            BotRenameSection(workspace: entry.workspace, bot: entry.bot) { to in
                // The page follows the Bot under its new name.
                nav.botRoute = BotRoute(workspace: entry.workspace.name, bot: to)
            }
            BotSettingsSections(draft: $draft, workspace: entry.workspace)
            Section {
                Button("Save") { Task { await save() } }
                    .disabled(draft.spec() == nil || !changed || model.settings.saving || model.settings.readOnly)
                    .accessibilityIdentifier("bot.settings.save")
                if changed {
                    Button("Revert") { draft = BotDraft(entry.bot) }
                        .accessibilityIdentifier("bot.settings.revert")
                }
                SaveError(text: error, id: "bot.settings.error")
            }
            Section {
                LabeledContent("Connections", value: names(entry.workspace.connections.map(\.name)))
                LabeledContent("MCP servers", value: names(entry.workspace.mcp.map(\.name)))
                LabeledContent("claude.ai connectors", value: entry.workspace.claudeConnectors ? "on" : "off")
                LabeledContent("Allowed", value: names(entry.workspace.botRules.allow))
                LabeledContent("Denied", value: names(entry.workspace.botRules.deny))
                LabeledContent("Its commands", value: names(entry.bot.spec.commands))
            } header: {
                Text("Access · \(entry.workspace.name)'s")
            } footer: {
                Text("A Bot has its Workspace's access and nothing else; change it in the Workspace's settings. "
                    + "Claude Code enforces the rules for this Workspace's Bots only.")
            }
            .accessibilityIdentifier("bot.access")
            BotRemovalSection(workspace: entry.workspace, bot: entry.bot) { dismiss() }
        }
        .onChange(of: entry.bot.spec) { _, _ in draft = BotDraft(entry.bot) }
    }

    private var changed: Bool {
        guard let spec = draft.spec() else { return true }
        return spec != entry.bot.spec
    }

    private func names(_ list: [String]) -> String { list.isEmpty ? "none" : list.joined(separator: ", ") }

    private func save() async {
        guard let spec = draft.spec() else { return }
        error = await model.settings.save([.botUpdate(workspace: entry.workspace.name, bot: spec)])
    }
}
