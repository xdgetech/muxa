import MuxaV2
import SwiftUI

/// A Bot's live chats: talk Tasks with its `muxa.bot` (ADR 0083). A closed
/// chat leaves no record, so only open ones show.
struct BotChatsTab: View {
    @Environment(NextModel.self) private var model
    @Environment(NextNavigation.self) private var nav
    let entry: BotEntry
    let newChat: () -> Void

    var body: some View {
        let talks = model.talks(of: entry)
        VStack(alignment: .leading, spacing: 8) {
            if talks.isEmpty {
                Text("No open chats.").foregroundStyle(Palette.textSecondary)
            }
            ForEach(talks) { t in
                Button { nav.open(t) } label: {
                    BotRow {
                        StateDot(color: t.state.color, size: 8)
                    } title: {
                        Text(t.title)
                    } subtitle: {
                        proposal(for: t) ? "Proposal waiting · \(t.botRole(of: entry.name))" : t.botRole(of: entry.name)
                    } trailing: {
                        Text("Continue in Focus")
                    }
                    .selectableRow(false)
                    .raisedCard(radius: 8)
                }
                .buttonStyle(.plain)
                .accessibilityIdentifier("bot.chat.\(t.id)")
            }
            Button("New chat", action: newChat)
                .buttonStyle(ActionButtonStyle(kind: .secondary, compact: true))
                .disabled(!model.isConnected)
                .accessibilityIdentifier("bot.chats.new")
            Text("Each chat is a Task with the Bot's soul and memory. Close it when the job is done; the next chat starts from what the Bot remembers.")
                .font(.system(size: 12))
                .foregroundStyle(Palette.textTertiary)
                .fixedSize(horizontal: false, vertical: true)
                .frame(maxWidth: 640, alignment: .leading)
        }
    }

    private func proposal(for t: MuxaV2.Terminal) -> Bool {
        model.inbox.items.contains { $0.terminal == t.id && $0.settingsProposal != nil }
    }
}

/// A Bot's Routines with their last run, on/off and Run now. Edit opens
/// Settings at the Routine.
struct BotRoutinesTab: View {
    @Environment(NextModel.self) private var model
    @Environment(MacChrome.self) private var chrome
    @Environment(\.openSettings) private var openSettings
    let entry: BotEntry

    var body: some View {
        let routines = model.routines(of: entry)
        VStack(alignment: .leading, spacing: 6) {
            if routines.isEmpty {
                Text("No Routines yet. A chat with \(entry.name) can propose them.").foregroundStyle(Palette.textSecondary)
            }
            ForEach(routines) { row($0) }
        }
    }

    private func row(_ r: Routine) -> some View {
        HStack(spacing: 12) {
            VStack(alignment: .leading, spacing: 2) {
                HStack(spacing: 8) {
                    Text(r.name).font(.system(size: 13.5, weight: .semibold))
                    if r.runningSince != nil { Badge(text: "Running", foreground: Palette.okText, background: Palette.okBack) }
                    if r.last?.ok == false { Badge(text: "Failed", foreground: Palette.late, background: Palette.warnBack) }
                }
                Text([r.timing, lastRun(r)].joined(separator: " · "))
                    .font(.system(size: 12))
                    .foregroundStyle(Palette.textTertiary)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .accessibilityElement(children: .combine)
            Toggle("On", isOn: Binding(get: { r.enabled }, set: { on in Task { await model.setRoutine(r, enabled: on) } }))
                .toggleStyle(.switch)
                .labelsHidden()
                .disabled(model.settings.readOnly || model.routineSettings(r) == nil || model.settings.saving)
                .help(r.enabled ? "Turn \(r.name) off" : "Turn \(r.name) on")
                .accessibilityIdentifier("bot.routine.on.\(r.name)")
            Button("Run now") { Task { await model.runRoutine(r) } }
                .buttonStyle(ActionButtonStyle(kind: .secondary, compact: true))
                .disabled(!r.enabled || !entry.bot.spec.enabled || r.runningSince != nil)
                .accessibilityIdentifier("bot.routine.run.\(r.name)")
            Button("Edit") {
                chrome.editRoutine = RoutineRef(workspace: r.workspace, name: r.name)
                openSettings()
            }
            .buttonStyle(ActionButtonStyle(kind: .secondary, compact: true))
            .accessibilityIdentifier("bot.routine.edit.\(r.name)")
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 8)
        .raisedCard(radius: 8)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("bot.routine.\(r.name)")
    }

    private func lastRun(_ r: Routine) -> String {
        guard let last = r.last else { return "never ran" }
        var parts = ["last \(routineStamp(last.at))", last.ok ? "OK" : "failed"]
        if let n = last.published { parts.append(n == 1 ? "1 item" : "\(n) items") }
        return parts.joined(separator: " ")
    }
}

/// A Bot's files: its Soul, which the user edits, and its Memory, which its
/// runs write, with the earlier ones to bring back.
struct BotFilesTab: View {
    @Environment(NextModel.self) private var model
    let entry: BotEntry
    @State private var editing: BotDraft?
    @State private var restoring: MemoryVersion?
    @State private var error: String?

    var body: some View {
        HStack(alignment: .top, spacing: 12) {
            panel(title: "Soul · yours", file: "soul.md") {
                text(entry.bot.soulText, empty: "No soul yet.", id: "bot.files.soul")
                Button("Edit") { editing = BotDraft(entry.bot) }
                    .buttonStyle(ActionButtonStyle(kind: .secondary, compact: true))
                    .disabled(model.settings.readOnly)
                    .accessibilityIdentifier("bot.files.editsoul")
            }
            panel(title: "Memory · the Bot's", file: entry.bot.memoryHistory.first.map { "updated \(routineStamp($0.at))" } ?? "memory.md") {
                text(entry.bot.memoryText, empty: "Nothing yet.", id: "bot.files.memory")
                if !entry.bot.memoryHistory.isEmpty {
                    SectionLabel(title: "Earlier versions").padding(.top, 6)
                    ForEach(entry.bot.memoryHistory) { v in
                        HStack {
                            Text("Version \(v.version) · replaced \(routineStamp(v.at))")
                                .frame(maxWidth: .infinity, alignment: .leading)
                            Button("Restore") { restoring = v }
                                .buttonStyle(ActionButtonStyle(kind: .secondary, compact: true))
                                .disabled(model.settings.readOnly || model.settings.saving)
                                .accessibilityIdentifier("bot.files.restore.\(v.version)")
                        }
                        .font(.system(size: 12.5))
                    }
                } else if entry.bot.memoryVersions > 0 {
                    Text("\(entry.bot.memoryVersions) earlier versions are kept. This host does not say when.")
                        .font(.system(size: 12))
                        .foregroundStyle(Palette.textTertiary)
                }
                SaveError(text: error, id: "bot.files.error")
            }
        }
        .sheet(item: $editing) { BotEditor(workspace: entry.workspace, draft: $0) }
        .confirmationDialog("Restore memory version \(restoring?.version ?? 0)?", isPresented: restoringBinding,
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

    private var restoringBinding: Binding<Bool> {
        Binding(get: { restoring != nil }, set: { if !$0 { restoring = nil } })
    }

    private func panel(title: String, file: String, @ViewBuilder content: () -> some View) -> some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack {
                SectionLabel(title: title)
                Spacer()
                Text(file).font(.system(size: 11.5, design: .monospaced)).foregroundStyle(Palette.textTertiary)
            }
            content()
        }
        .padding(14)
        .frame(maxWidth: .infinity, alignment: .leading)
        .raisedCard()
    }

    private func text(_ value: String?, empty: String, id: String) -> some View {
        Text(value.map { $0.isEmpty ? empty : $0 } ?? "It cannot be read.")
            .font(.system(size: 12.5, design: .monospaced))
            .lineSpacing(3)
            .foregroundStyle(Palette.terminalText)
            .textSelection(.enabled)
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(10)
            .background(Palette.terminal, in: RoundedRectangle(cornerRadius: 8))
            .accessibilityIdentifier(id)
    }
}
