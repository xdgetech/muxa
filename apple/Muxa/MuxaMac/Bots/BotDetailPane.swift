import MuxaV2
import SwiftUI

/// The picked Bot: its status, who it is, the Home/Chats/Routines/Files tabs
/// and New chat, Run now and Pause at the foot (ADR 0086). Chats fills the
/// pane with its own split instead.
struct BotDetailPane: View {
    @Environment(NextModel.self) private var model
    @Environment(NextNavigation.self) private var nav
    @Environment(MacChrome.self) private var chrome
    let entry: BotEntry
    @State private var choosingChat = false

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            heading
            if chrome.botTab == .chats {
                BotChatsTab(entry: entry, newChat: { Task { await startChat() } })
                    .padding(.horizontal, 28)
                    .padding(.vertical, 14)
            } else {
                ScrollView {
                    Group {
                        switch chrome.botTab {
                        case .home, .chats: BotHomeTab(entry: entry, newChat: newChat)
                        case .routines: BotRoutinesTab(entry: entry)
                        case .files: BotFilesTab(entry: entry)
                        }
                    }
                    .frame(maxWidth: 900, alignment: .leading)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.horizontal, 28)
                    .padding(.vertical, 18)
                }
                actions
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        .background(Palette.pane)
        .confirmationDialog("Continue a chat with \(entry.name)?", isPresented: $choosingChat, titleVisibility: .visible) {
            ForEach(model.talks(of: entry)) { t in
                Button("Continue \(t.title)") { nav.open(t) }
            }
            Button("Start a New Chat") { Task { await startChat() } }
                .accessibilityIdentifier("bot.chat.new")
        } message: {
            Text("A chat keeps its Task until you close it.")
        }
    }

    private var heading: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack(spacing: 8) {
                let status = entry.work.status
                Badge(text: status.title, foreground: status.badgeForeground, background: status.badgeBackground)
                    .accessibilityIdentifier("bot.status")
                Text(meta).font(.system(size: 12)).foregroundStyle(Palette.textSecondary).lineLimit(1)
            }
            HStack(spacing: 10) {
                BotMark(paused: !entry.bot.spec.enabled, size: 30)
                Text(entry.name).font(.system(size: 21, weight: .bold)).accessibilityIdentifier("bot.title")
            }
            if let d = entry.bot.spec.description {
                Text(d).foregroundStyle(Palette.textSecondary)
            }
            if model.settings.readOnly {
                Label(SettingsModel.lockedNotice, systemImage: "lock")
                    .font(.system(size: 12))
                    .foregroundStyle(Theme.attention)
                    .fixedSize(horizontal: false, vertical: true)
                    .accessibilityIdentifier("bot.readonly")
            }
            UnderlineTabs(tabs: BotTab.allCases.map { ($0.title, $0, "bot.tab.\($0.title.lowercased())") },
                          selection: Bindable(chrome).botTab)
                .padding(.top, 4)
        }
        .padding(.horizontal, 28)
        .padding(.top, 18)
    }

    /// `app · 3 items a day · reviewer · leads research, writer`.
    private var meta: String {
        let spec = entry.bot.spec
        var parts = [entry.workspace.name, "\(spec.dailyItems ?? 3) items a day", spec.worker ?? "the Workspace's agent"]
        if !spec.members.isEmpty { parts.append("leads \(spec.members.joined(separator: ", "))") }
        if let lead = entry.workspace.bots.first(where: { $0.spec.members.contains(entry.name) }) {
            parts.append("member of \(lead.name)")
        }
        return parts.joined(separator: " · ")
    }

    // MARK: Actions

    /// The Routine Run now starts: the next planned one, else the Bot's first.
    private var runnable: Routine? {
        entry.work.planned.first { $0.nextDue != nil } ?? model.routines(of: entry).first
    }

    private var actions: some View {
        let paused = !entry.bot.spec.enabled
        let routine = runnable
        return ActionBar {
            Button(action: newChat) { ActionLabel(title: "New chat", key: Shortcut.newChat.display, primary: true) }
                .buttonStyle(ActionButtonStyle(kind: .primary))
                .keyboardShortcut("t", modifiers: .command)
                .disabled(!model.isConnected)
                .accessibilityIdentifier("bot.newchat")
            Button { if let routine { Task { await model.runRoutine(routine) } } } label: {
                ActionLabel(title: routine.map { "Run \($0.name) now" } ?? "Run now", key: Shortcut.runBot.display)
            }
            .buttonStyle(ActionButtonStyle())
            .keyboardShortcut("r", modifiers: .command)
            .disabled(paused || routine == nil || routine?.runningSince != nil)
            .help(runHelp(paused: paused, routine: routine))
            .accessibilityIdentifier("bot.run")
            Button(paused ? "Resume" : "Pause") { Task { await model.setBot(entry, enabled: paused) } }
                .buttonStyle(ActionButtonStyle())
                .disabled(model.settings.readOnly || model.settings.saving)
                .accessibilityIdentifier("bot.pause")
            Spacer(minLength: 0)
        }
    }

    private func runHelp(paused: Bool, routine: Routine?) -> String {
        if paused { return "\(entry.name) is paused. Resume it to run its Routines." }
        guard let routine else { return "\(entry.name) has no Routines yet." }
        if routine.runningSince != nil { return "\(routine.name) runs now." }
        return "Run \(routine.name) now (\(Shortcut.runBot.display))"
    }

    /// New chat: an open chat is offered first.
    private func newChat() {
        if model.talks(of: entry).isEmpty {
            Task { await startChat() }
        } else {
            choosingChat = true
        }
    }

    private func startChat() async {
        if let t = await model.startChat(with: entry) { nav.open(t) }
    }
}
