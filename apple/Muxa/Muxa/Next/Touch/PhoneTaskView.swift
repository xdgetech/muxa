import MuxaV2
import SwiftUI

/// One Task on a phone (PhoneTab mock): back, the Task's place and title,
/// Chat | Terminal | Reader, who needs you, the terminal, its keys and a line.
struct PhoneTaskView: View {
    @Environment(NextModel.self) private var model
    @Environment(NextNavigation.self) private var nav
    let terminal: MuxaV2.Terminal
    @State private var chat: UInt32?
    @State private var showLoop = false
    @State private var showInbox = false
    @FocusState private var lineFocused: Bool

    var body: some View {
        VStack(spacing: 0) {
            header
            TaskSegments(terminal: terminal)
                .padding(.horizontal, 12)
                .padding(.bottom, 10)
            if nav.view(for: terminal) != .chat {
                NeedsYouBar(terminal: terminal, prompt: chat.flatMap { model.chatPrompts[$0] })
                    .padding(.horizontal, 12)
                    .padding(.bottom, 10)
            }
            TaskBody(terminal: terminal, chat: $chat, keyboardInFooter: lineFocused) {
                VStack(spacing: 6) {
                    KeyRow(terminal: terminal.id)
                    TerminalLine(terminal: terminal.id, focused: $lineFocused)
                }
                .padding(.horizontal, 8)
                .padding(.top, 10)
                .padding(.bottom, 4)
            }
        }
        .background(Palette.ground)
        .background { if terminal.isTaskAgent { Color.clear.chatLink(terminal.id, chat: $chat) } }
        .sheet(isPresented: $showLoop) { LoopSheet(terminal: terminal) }
        .sheet(isPresented: $showInbox) { InboxSheet() }
    }

    private var header: some View {
        HStack(spacing: 4) {
            Button { nav.focus = false } label: { Image(systemName: "chevron.left") }
                .buttonStyle(BareIconStyle())
                .accessibilityLabel("Back")
                .accessibilityIdentifier("mode.home")
            VStack(alignment: .leading, spacing: 1) {
                Text(terminal.pathText).font(.system(size: 12)).foregroundStyle(Palette.textSecondary).lineLimit(1)
                HStack(spacing: 6) {
                    Text(terminal.title).font(.system(size: 16, weight: .semibold)).foregroundStyle(Palette.text).lineLimit(1)
                    if let bot = terminal.bot { BotChip(bot: bot, label: "talks with", id: "task.bot").font(.caption) }
                }
            }
            Spacer(minLength: 4)
            UploadButton(terminal: terminal.id)
                .buttonStyle(BareIconStyle(color: Palette.textSecondary))
                .disabled(!terminal.isTaskAgent)
            LoopButton(terminal: terminal) { showLoop = true }
            Button { showInbox = true } label: { Image(systemName: "tray") }
                .buttonStyle(BareIconStyle(color: Palette.textSecondary))
                .accessibilityLabel("Inbox")
                .accessibilityIdentifier("focus.inbox")
        }
        .padding(.horizontal, 8)
        .padding(.top, 4)
        .padding(.bottom, 8)
    }
}

/// The Inbox over a Task, closed with a button or a swipe down.
struct InboxSheet: View {
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            InboxPanel()
                .inlineTitle()
                .toolbar {
                    ToolbarItem(placement: .cancellationAction) { Button("Close") { dismiss() } }
                }
        }
        .presentationDetents([.medium, .large])
        .preferredColorScheme(.dark)
    }
}
