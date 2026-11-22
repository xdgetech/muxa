import MuxaV2
import SwiftUI

/// One Task in the tablet's main area (Tablet mock): the place and title,
/// Chat | Reader | Terminal, and the reply bar under the terminal and the Reader.
struct TabletTaskView: View {
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
            TaskBody(terminal: terminal, chat: $chat, keyboardInFooter: lineFocused) { EmptyView() }
                .padding(.top, 8)
            if nav.view(for: terminal) != .chat {
                ReplyBar(terminal: terminal, prompt: chat.flatMap { model.chatPrompts[$0] }, focused: $lineFocused)
            }
        }
        .background(Palette.ground)
        .background { if terminal.isTaskAgent { Color.clear.chatLink(terminal.id, chat: $chat) } }
        .sheet(isPresented: $showLoop) { LoopSheet(terminal: terminal) }
        .sheet(isPresented: $showInbox) { InboxSheet() }
    }

    private var header: some View {
        HStack(spacing: 12) {
            VStack(alignment: .leading, spacing: 4) {
                Text(path).font(.system(size: 13)).foregroundStyle(Palette.textSecondary).lineLimit(1)
                Text(terminal.title).font(.system(size: 19, weight: .semibold)).foregroundStyle(Palette.text).lineLimit(1)
            }
            Spacer(minLength: 8)
            LoopButton(terminal: terminal) { showLoop = true }
            Button { showInbox = true } label: { Image(systemName: "tray") }
                .buttonStyle(BareIconStyle(color: Palette.textSecondary))
                .accessibilityLabel("Inbox")
                .accessibilityIdentifier("focus.inbox")
            TaskSegments(terminal: terminal, order: [.chat, .reader, .terminal])
                .frame(maxWidth: 300)
        }
        .padding(.horizontal, 24)
        .padding(.top, 18)
        .padding(.bottom, 14)
        .overlay(alignment: .bottom) { Rectangle().fill(Palette.border).frame(height: 1) }
    }

    /// `app · app · branch · claude-code`: the Workspace, the place and the program.
    private var path: String {
        [terminal.pathText, terminal.status?.app].compactMap { $0 }.joined(separator: " · ")
    }
}

/// The bar under a Task: the menu the agent waits on, a line for it, and Approve.
private struct ReplyBar: View {
    @Environment(NextModel.self) private var model
    @Environment(NextNavigation.self) private var nav
    let terminal: MuxaV2.Terminal
    let prompt: ChatPrompt?
    var focused: FocusState<Bool>.Binding
    @State private var text = ""

    var body: some View {
        VStack(spacing: 10) {
            if let prompt { promptLine(prompt) }
            HStack(spacing: 10) {
                TextField(prompt == nil ? "Type a line for the terminal" : "Reply, or tell it what to change", text: $text)
                    .focused(focused)
                    .submitLabel(.send)
                    .onSubmit(send)
                    .fieldWell(radius: 10, ground: Palette.ground)
                    .accessibilityIdentifier("focus.line")
                if let option = prompt?.approve {
                    ApproveButton(option: option, terminal: terminal.id)
                } else {
                    SendButton(disabled: text.isEmpty, action: send).accessibilityIdentifier("focus.send")
                }
            }
        }
        .padding(.horizontal, 24)
        .padding(.top, 12)
        .padding(.bottom, 16)
        .background(Palette.surface)
        .overlay(alignment: .top) { Rectangle().fill(Palette.border).frame(height: 1) }
    }

    private func promptLine(_ prompt: ChatPrompt) -> some View {
        HStack(spacing: 10) {
            StateDot(color: TaskState.needsYou.color, size: 8)
            Text(Self.line(prompt))
                .font(.system(size: 12, design: .monospaced))
                .foregroundStyle(Palette.textSecondary)
                .lineLimit(2)
            Spacer(minLength: 0)
            if nav.view(for: terminal) != .terminal {
                Button("Open Terminal") { nav.taskView = .terminal }
                    .font(.system(size: 13))
                    .frame(minHeight: minTouch)
            }
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 8)
        .frame(minHeight: 36)
        .background(Palette.ground, in: RoundedRectangle(cornerRadius: 10))
        .overlay(RoundedRectangle(cornerRadius: 10).stroke(Palette.border))
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("focus.needsyou")
    }

    /// `? Allow edits?  › 1. Allow  2. Deny`
    static func line(_ prompt: ChatPrompt) -> String {
        let options = prompt.options.enumerated().map { "\($0.offset + 1). \($0.element.label)" }
        return "? \(prompt.question ?? prompt.title)  › \(options.joined(separator: "  "))"
    }

    private func send() {
        guard !text.isEmpty else { return }
        let line = text
        text = ""
        Task { await model.send(line + "\r", to: terminal.id) }
    }
}
