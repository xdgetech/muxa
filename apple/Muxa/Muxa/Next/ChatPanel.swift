import MuxaV2
import SwiftUI

/// Chat beside a terminal (plan F4): the conversation of a Task's agent from
/// its own records, the choice it waits on, and a line to send it. Answers
/// go to the terminal as input; muxa never answers a menu by itself.
struct ChatPanel: View {
    @Environment(NextModel.self) private var model
    let terminal: MuxaV2.Terminal
    @State private var chat: UInt32?
    @State private var draft = ""

    var body: some View {
        NavigationStack {
            Group {
                if let chat, let reason = model.chatEnded[chat] {
                    ContentUnavailableView {
                        Label("The chat ended", systemImage: "bubble.left.and.exclamationmark.bubble.right")
                    } description: {
                        Text(reason)
                    } actions: {
                        Button("Open Again") { Task { await open() } }
                    }
                } else if let chat, let entries = model.chats[chat] {
                    conversation(entries, prompt: model.chatPrompts[chat])
                } else {
                    ProgressView()
                }
            }
            .navigationTitle("Chat")
            .inlineTitle()
        }
        .task(id: terminal.id) { await open() }
        .onDisappear { close() }
    }

    private func conversation(_ entries: [ChatEntry], prompt: ChatPrompt?) -> some View {
        VStack(spacing: 0) {
            ScrollViewReader { proxy in
                List {
                    ForEach(entries) { ChatRow(entry: $0) }
                    Color.clear.frame(height: 1).id("end").listRowSeparator(.hidden)
                }
                .listStyle(.plain)
                .accessibilityIdentifier("chat.list")
                .onAppear { proxy.scrollTo("end") }
                .onChange(of: entries.count) { proxy.scrollTo("end") }
            }
            if let prompt { PromptCard(prompt: prompt) { send($0) } }
            HStack {
                TextField("Message the agent", text: $draft)
                    .textFieldStyle(.roundedBorder)
                    .onSubmit(sendDraft)
                    .accessibilityIdentifier("chat.field")
                Button("Send", systemImage: "paperplane", action: sendDraft)
                    .labelStyle(.iconOnly)
                    .disabled(draft.trimmingCharacters(in: .whitespaces).isEmpty)
                    .accessibilityIdentifier("chat.send")
            }
            .padding(8)
            if let error = model.requestError {
                Text(error).font(.caption).foregroundStyle(.red).padding(.bottom, 4)
            }
        }
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

    private func open() async {
        close()
        chat = await model.openChat(terminal.id)
    }

    private func close() {
        guard let n = chat else { return }
        chat = nil
        Task { await model.closeChat(n) }
    }
}

private struct ChatRow: View {
    let entry: ChatEntry

    var body: some View {
        switch entry.kind {
        case "user":
            Text(entry.text ?? "")
                .padding(8)
                .background(.tint.opacity(0.15), in: RoundedRectangle(cornerRadius: 8))
                .frame(maxWidth: .infinity, alignment: .trailing)
                .accessibilityIdentifier("chat.user")
        case "tool":
            VStack(alignment: .leading, spacing: 2) {
                Label("\(entry.tool?.name ?? "Tool") \(entry.tool?.summary ?? "")", systemImage: "wrench.and.screwdriver")
                    .font(.caption.monospaced())
                    .lineLimit(2)
                if let result = entry.tool?.result {
                    Text(result).font(.caption2.monospaced()).foregroundStyle(entry.tool?.error == true ? .red : .secondary)
                        .lineLimit(4)
                }
            }
            .accessibilityElement(children: .combine)
            .accessibilityIdentifier("chat.tool")
        case "notice":
            Text(entry.text ?? "").font(.caption).foregroundStyle(.secondary)
        default:
            Text(markdown(entry.text ?? ""))
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
            Text(title).font(.headline)
            if let q = prompt.question { Text(q) }
            HStack {
                ForEach(prompt.options) { o in
                    Button(o.label, role: o.cancel == true ? .cancel : nil) { pick(o.input) }
                        .buttonStyle(.bordered)
                        .accessibilityIdentifier("chat.option.\(o.label)")
                }
            }
        }
        .padding(10)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(.orange.opacity(0.12))
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("chat.prompt")
    }

    private var title: String {
        switch prompt.kind {
        case "permission": "The agent asks to go on"
        case "question": "The agent asks"
        case "plan": "The agent asks to leave plan mode"
        default: "The agent waits"
        }
    }
}
