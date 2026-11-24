import MuxaV2
import SwiftUI

/// One of a Bot's talks, runs or members on its page (ADR 0086): Chat,
/// Terminal and Reader, Close for a talk, the talk's proposal above, and
/// what the agent waits on. The terminal stays attached under Chat, so
/// answers reach it. A run or member has no Chat: it opens on Terminal.
struct BotConversationScreen: View {
    @Environment(NextModel.self) private var model
    @Environment(NextNavigation.self) private var nav
    @Environment(\.dismiss) private var dismiss
    let terminal: MuxaV2.Terminal
    @State private var chat: UInt32?
    @State private var closing = false
    @State private var wouldLoseWork: String?
    @FocusState private var lineFocused: Bool

    var body: some View {
        VStack(spacing: 0) {
            if let item = proposalItem, let p = item.settingsProposal {
                ProposalBar(item: item, proposal: p).id(item.id)
            }
            TaskSegments(terminal: terminal)
                .padding(.horizontal, 12)
                .padding(.vertical, 8)
            if nav.view(for: terminal) != .chat {
                NeedsYouBar(terminal: terminal, prompt: chat.flatMap { model.chatPrompts[$0] })
                    .padding(.horizontal, 12)
                    .padding(.bottom, 8)
            }
            TaskBody(terminal: terminal, chat: $chat, keyboardInFooter: lineFocused) {
                VStack(spacing: 6) {
                    KeyRow(terminal: terminal.id)
                    TerminalLine(terminal: terminal.id, focused: $lineFocused)
                }
                .padding(.horizontal, 10)
                .padding(.vertical, 8)
            }
            if let error = model.requestError {
                Text(error)
                    .font(.footnote)
                    .foregroundStyle(Palette.late)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.horizontal, 14)
                    .padding(.vertical, 6)
                    .accessibilityIdentifier("bot.conversation.error")
            }
        }
        .background(Palette.ground)
        // Follows the talk's conversation, as a Task screen does; without it Chat never loads.
        .background { if terminal.isTaskAgent { Color.clear.chatLink(terminal.id, chat: $chat) } }
        .navigationTitle(terminal.title)
        .inlineTitle()
        .toolbar {
            if terminal.isTaskAgent {
                ToolbarItem(placement: .primaryAction) {
                    Button("Close", systemImage: "xmark.circle") { closing = true }
                        .accessibilityIdentifier("bot.chat.close")
                }
            }
        }
        // A container keeps its children's own identifiers (chat.field…).
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("bot.conversation")
        // A talk opens on Chat; a run or member, which has none, on Terminal.
        .onAppear { nav.taskView = terminal.isTaskAgent ? .chat : .terminal }
        .confirmationDialog("Close “\(terminal.title)”?", isPresented: $closing, titleVisibility: .visible) {
            Button("Close Chat", role: .destructive) { Task { await close(force: false) } }
                .accessibilityIdentifier("closetask.confirm")
        } message: {
            Text("Its agent stops. What it learned stays in the Bot's memory.")
        }
        .alert("Remove the worktree anyway?", isPresented: Binding(get: { wouldLoseWork != nil },
                                                                  set: { if !$0 { wouldLoseWork = nil } }),
               presenting: wouldLoseWork) { _ in
            Button("Remove Anyway", role: .destructive) { Task { await close(force: true) } }
            Button("Cancel", role: .cancel) {}
        } message: { Text($0) }
    }

    private var proposalItem: InboxItem? {
        model.inbox.items.first { $0.terminal == terminal.id && $0.settingsProposal != nil }
    }

    private func close(force: Bool) async {
        if let why = await model.closeTask(terminal.id, force: force) {
            wouldLoseWork = why
        } else if model.requestError == nil {
            dismiss()
        }
    }
}
