import MuxaV2
import SwiftUI

/// A Bot's chats on its own page (ADR 0086): its live talks on the left with
/// New chat, the picked one on the right. A run or a member picked on Home
/// opens here too, on Terminal. A closed chat leaves no record, so only
/// live ones show.
struct BotChatsTab: View {
    @Environment(NextModel.self) private var model
    @Environment(MacChrome.self) private var chrome
    let entry: BotEntry
    let newChat: () -> Void

    var body: some View {
        let talks = model.talks(of: entry)
        let runs = entry.work.terminals.filter { !talks.contains($0) }
        HStack(spacing: 0) {
            list(talks: talks, runs: runs)
                .frame(width: 240)
                .frame(maxHeight: .infinity, alignment: .top)
                .background(Palette.surface)
                .overlay(alignment: .trailing) { Rectangle().fill(Palette.border).frame(width: 1) }
            if let t = open(talks + runs) {
                BotConversation(entry: entry, terminal: t).id(t.id)
            } else {
                ContentUnavailableView {
                    Label("No chat open", systemImage: "bubble.left.and.bubble.right")
                } description: {
                    Text("Start a chat with \(entry.name), or pick one on the left.")
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
        .clipShape(RoundedRectangle(cornerRadius: 8))
        .overlay(RoundedRectangle(cornerRadius: 8).strokeBorder(Palette.border))
    }

    /// The picked terminal while it is still live, else the first talk.
    private func open(_ terminals: [MuxaV2.Terminal]) -> MuxaV2.Terminal? {
        terminals.first { $0.id == chrome.botTerminal } ?? terminals.first { $0.isTaskAgent }
    }

    private func list(talks: [MuxaV2.Terminal], runs: [MuxaV2.Terminal]) -> some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 2) {
                Button(action: newChat) { ActionLabel(title: "New chat", key: Shortcut.newChat.display, primary: true) }
                    .buttonStyle(ActionButtonStyle(kind: .primary, compact: true))
                    .keyboardShortcut("t", modifiers: .command)
                    .disabled(!model.isConnected)
                    .frame(maxWidth: .infinity)
                    .padding(.bottom, 6)
                    .accessibilityIdentifier("bot.chats.new")
                if talks.isEmpty {
                    Text("No open chats.").font(.system(size: 12)).foregroundStyle(Palette.textTertiary).padding(8)
                }
                ForEach(talks) { row($0) }
                if !runs.isEmpty {
                    SectionLabel(title: "Runs", count: runs.count).padding(.horizontal, 8).padding(.top, 10).padding(.bottom, 2)
                    ForEach(runs) { row($0) }
                }
            }
            .padding(8)
        }
    }

    private func row(_ t: MuxaV2.Terminal) -> some View {
        let picked = t.id == open(model.talks(of: entry) + entry.work.terminals)?.id
        let waiting = proposal(for: t)
        return Button {
            chrome.botTerminal = t.id
            chrome.botView = t.isTaskAgent ? .chat : .terminal
        } label: {
            HStack(spacing: 8) {
                StateDot(color: waiting ? Palette.accent : t.state.color, size: 7)
                VStack(alignment: .leading, spacing: 2) {
                    Text(t.title).font(.system(size: 13, weight: .medium)).lineLimit(1)
                    Text(waiting ? "proposal waiting" : t.botRole(of: entry.name))
                        .font(.system(size: 11.5))
                        .foregroundStyle(waiting ? Palette.accent : Palette.textTertiary)
                        .lineLimit(1)
                }
                Spacer(minLength: 0)
            }
            .padding(.horizontal, 8)
            .padding(.vertical, 6)
            .selectableRow(picked)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityElement(children: .combine)
        .accessibilityAddTraits(picked ? [.isButton, .isSelected] : .isButton)
        .accessibilityIdentifier("bot.chat.\(t.id)")
    }

    private func proposal(for t: MuxaV2.Terminal) -> Bool {
        model.inbox.items.contains { $0.terminal == t.id && $0.settingsProposal != nil }
    }
}

/// One talk, run or member on its Bot's page: a bar with its state, Chat,
/// Terminal and Reader, and Close; the talk's proposal above; then the view.
/// Chat, the terminal and the Reader are the same views Focus uses.
struct BotConversation: View {
    @Environment(NextModel.self) private var model
    @Environment(MacChrome.self) private var chrome
    let entry: BotEntry
    let terminal: MuxaV2.Terminal

    var body: some View {
        VStack(spacing: 0) {
            bar
            if let item = proposalItem, let p = item.settingsProposal {
                ProposalBar(item: item, proposal: p).id(item.id)
            }
            content
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            if let error = model.requestError {
                // A send that could not reach the agent says so here, where it was typed.
                Text(error)
                    .font(.system(size: 12))
                    .foregroundStyle(Palette.late)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.horizontal, 12)
                    .padding(.vertical, 6)
                    .background(Palette.surface)
                    .accessibilityIdentifier("bot.conversation.error")
            }
        }
        .background(Palette.ground)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("bot.conversation")
    }

    /// Chat serves a Task's agent only; a run or a member answers on Terminal.
    private var view: BotView {
        chrome.botView == .chat && !terminal.isTaskAgent ? .terminal : chrome.botView
    }

    /// The terminal stays attached under Chat and the Reader, at the pane's
    /// size: Chat's answers are typed into its channel, which exists only while
    /// the terminal is attached.
    private var content: some View {
        ZStack {
            TerminalPane(terminal: terminal, active: view == .terminal).id(terminal.id)
                .allowsHitTesting(view == .terminal)
                .accessibilityHidden(view != .terminal)
            switch view {
            case .chat:
                ChatPanel(terminal: terminal).id("chat.\(terminal.id)")
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                    .background(Palette.surface)
            case .reader:
                ReaderPanel(terminal: terminal).id("reader.\(terminal.id)")
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                    .background(Palette.surface)
            case .terminal:
                EmptyView()
            }
        }
    }

    private var proposalItem: InboxItem? {
        model.inbox.items.first { $0.terminal == terminal.id && $0.settingsProposal != nil }
    }

    private var bar: some View {
        HStack(spacing: 8) {
            StateDot(color: terminal.state.color, size: 8)
            Text(terminal.title).fontWeight(.semibold).lineLimit(1)
            Text([terminal.status?.app, terminal.status.map { TaskState($0.state).title.lowercased() }]
                .compactMap { $0 }.joined(separator: " · "))
                .foregroundStyle(Palette.textSecondary)
                .lineLimit(1)
            Spacer(minLength: 8)
            if terminal.isTaskAgent {
                segment("Chat", systemImage: "bubble.left.and.text.bubble.right", .chat)
            }
            segment("Terminal", systemImage: "terminal", .terminal)
            segment("Reader", systemImage: "doc.text.magnifyingglass", .reader)
                .disabled(terminal.taskDir == nil)
            if terminal.isTaskAgent {
                Button("Close", systemImage: "xmark") { chrome.closingTask = terminal }
                    .buttonStyle(PaneActionStyle(on: false))
                    .help("Close this chat")
                    .accessibilityIdentifier("bot.chat.close")
            }
        }
        .font(.system(size: 12))
        .padding(.horizontal, 12)
        .frame(height: 34)
        .background(Palette.surface)
        .overlay(alignment: .bottom) { Rectangle().fill(Palette.border).frame(height: 1) }
    }

    private func segment(_ title: String, systemImage: String, _ tag: BotView) -> some View {
        Button(title, systemImage: systemImage) { chrome.botView = tag }
            .buttonStyle(PaneActionStyle(on: view == tag))
            .accessibilityIdentifier("bot.view.\(title.lowercased())")
    }
}
