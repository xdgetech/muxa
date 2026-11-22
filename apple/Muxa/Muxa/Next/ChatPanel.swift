import MuxaV2
import SwiftUI

/// Keeps a Task agent's chat open while the view is shown. The Task screen
/// holds it, so the Needs-you bar sees the prompt also on the Terminal segment.
private struct ChatLink: ViewModifier {
    @Environment(NextModel.self) private var model
    let terminal: String
    @Binding var chat: UInt32?

    func body(content: Content) -> some View {
        content
            .task(id: terminal) {
                close()
                chat = await model.openChat(terminal)
            }
            .onDisappear { close() }
    }

    private func close() {
        guard let n = chat else { return }
        chat = nil
        Task { await model.closeChat(n) }
    }
}

extension View {
    func chatLink(_ terminal: String, chat: Binding<UInt32?>) -> some View {
        modifier(ChatLink(terminal: terminal, chat: chat))
    }
}

/// Chat with its own conversation: the Mac's panel beside Focus.
struct ChatPanel: View {
    let terminal: MuxaV2.Terminal
    @State private var chat: UInt32?

    var body: some View {
        ChatView(terminal: terminal, chat: $chat).chatLink(terminal.id, chat: $chat)
    }
}

/// Chat (plan F4): the conversation of a Task's agent from its own records,
/// the choice it waits on, and a line to send it. Answers go to the terminal
/// as input; muxa never answers a menu by itself.
struct ChatView: View {
    @Environment(NextModel.self) private var model
    let terminal: MuxaV2.Terminal
    @Binding var chat: UInt32?
    @State private var draft = ""

    var body: some View {
        Group {
            if let chat, let reason = model.chatEnded[chat] {
                ContentUnavailableView {
                    Label("The chat ended", systemImage: "bubble.left.and.exclamationmark.bubble.right")
                } description: {
                    Text(reason)
                } actions: {
                    Button("Open Again") { Task { await reopen() } }
                }
            } else if let chat, let entries = model.chats[chat] {
                conversation(entries, prompt: model.chatPrompts[chat])
            } else {
                ProgressView().frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
        .background(Palette.ground)
    }

    private func conversation(_ entries: [ChatEntry], prompt: ChatPrompt?) -> some View {
        VStack(spacing: 0) {
            ScrollViewReader { proxy in
                List {
                    ForEach(entries) {
                        ChatRow(entry: $0).listRowBackground(Color.clear).listRowSeparator(.hidden)
                    }
                    Color.clear.frame(height: 1).id("end").listRowBackground(Color.clear).listRowSeparator(.hidden)
                }
                .listStyle(.plain)
                .scrollContentBackground(.hidden)
                .accessibilityIdentifier("chat.list")
                .onAppear { proxy.scrollTo("end") }
                .onChange(of: entries.count) { proxy.scrollTo("end") }
            }
            if let prompt { PromptCard(prompt: prompt) { send($0) } }
            composer
            if let error = model.requestError {
                Text(error).font(.caption).foregroundStyle(Palette.late).padding(.bottom, 4)
            }
        }
    }

    private var composer: some View {
        HStack(spacing: 8) {
            TextField("Message the agent", text: $draft)
                .onSubmit(sendDraft)
                .padding(.horizontal, 4)
                .fieldWell(radius: minTouch / 2)
                .accessibilityIdentifier("chat.field")
            SendButton(disabled: draft.trimmingCharacters(in: .whitespaces).isEmpty, action: sendDraft)
                .accessibilityIdentifier("chat.send")
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 8)
    }

    private func sendDraft() {
        let text = draft.trimmingCharacters(in: .whitespaces)
        guard !text.isEmpty else { return }
        draft = ""
        send(text + "\r")
    }

    private func send(_ input: String) {
        Task { await model.send(input, to: terminal.id) }
    }

    private func reopen() async {
        if let n = chat { await model.closeChat(n) }
        chat = await model.openChat(terminal.id)
    }
}

private struct ChatRow: View {
    let entry: ChatEntry

    var body: some View {
        switch entry.kind {
        case "user":
            Text(entry.text ?? "")
                .foregroundStyle(Palette.text)
                .padding(.horizontal, 12)
                .padding(.vertical, 8)
                .background(Palette.raised, in: RoundedRectangle(cornerRadius: 12))
                .frame(maxWidth: .infinity, alignment: .trailing)
                .accessibilityIdentifier("chat.user")
        case "tool":
            VStack(alignment: .leading, spacing: 2) {
                Label("\(entry.tool?.name ?? "Tool") \(entry.tool?.summary ?? "")", systemImage: "wrench.and.screwdriver")
                    .font(.caption.monospaced())
                    .foregroundStyle(Palette.textSecondary)
                    .lineLimit(2)
                if let result = entry.tool?.result {
                    Text(result).font(.caption2.monospaced())
                        .foregroundStyle(entry.tool?.error == true ? Palette.late : Palette.textTertiary)
                        .lineLimit(4)
                }
            }
            .padding(10)
            .frame(maxWidth: .infinity, alignment: .leading)
            .card(radius: 10, border: true)
            .accessibilityElement(children: .combine)
            .accessibilityIdentifier("chat.tool")
        case "notice":
            Text(entry.text ?? "").font(.caption).foregroundStyle(Palette.textTertiary)
        default:
            Text(markdown(entry.text ?? ""))
                .foregroundStyle(Palette.text)
                .textSelection(.enabled)
                .accessibilityIdentifier("chat.assistant")
        }
    }

    private func markdown(_ text: String) -> AttributedString {
        let options = AttributedString.MarkdownParsingOptions(interpretedSyntax: .inlineOnlyPreservingWhitespace)
        return (try? AttributedString(markdown: text, options: options)) ?? AttributedString(text)
    }
}

/// The choice the agent waits on, as buttons that send the checked input.
private struct PromptCard: View {
    let prompt: ChatPrompt
    var pick: (String) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 8) {
                StateDot(color: TaskState.needsYou.color)
                Text(prompt.title).font(.headline).foregroundStyle(Palette.text)
            }
            if let q = prompt.question { Text(q).foregroundStyle(Palette.text) }
            HStack {
                ForEach(prompt.options) { o in
                    if o.cancel == true {
                        Button(o.label, role: .cancel) { pick(o.input) }
                            .buttonStyle(QuietButtonStyle())
                            .accessibilityIdentifier("chat.option.\(o.label)")
                    } else {
                        Button(o.label) { pick(o.input) }
                            .buttonStyle(FilledButtonStyle(fill: TaskState.needsYou.color, text: Palette.onAmber))
                            .accessibilityIdentifier("chat.option.\(o.label)")
                    }
                }
            }
        }
        .padding(12)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Palette.promptBack, in: RoundedRectangle(cornerRadius: 12))
        .overlay(RoundedRectangle(cornerRadius: 12).stroke(Palette.promptBorder))
        .padding(.horizontal, 10)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("chat.prompt")
    }
}

extension ChatPrompt {
    /// What the agent waits for, in a few words.
    var title: String {
        switch kind {
        case "permission": "The agent asks to go on"
        case "question": "The agent asks"
        case "plan": "The agent asks to leave plan mode"
        default: "The agent waits"
        }
    }

    /// The option a single Approve button sends: the first that goes on.
    var approve: ChatOption? { options.first { $0.cancel != true } }
}
