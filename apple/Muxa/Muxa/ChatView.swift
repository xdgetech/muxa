import MarkdownUI
import MuxaProtocol
import MuxaSession
import MuxaUI
import SwiftUI

/// The Pretty view of an agent pane: the conversation as messages and a composer.
/// Everything sent still goes to the same pane, as a paste and Enter.
struct ChatView: View {
    @Environment(AppModel.self) private var model
    let log: ChatLog?

    var body: some View {
        VStack(spacing: 0) {
            messages
            if let log, log.status?.state == .waiting {
                if let prompt = log.prompt {
                    ApprovalCard(prompt: prompt, tool: log.entries.last { $0.id == prompt.entry }?.tool)
                        .id(prompt.entry)
                } else {
                    WaitingBanner()
                }
            }
            if let ended = log?.ended {
                EndedBanner(reason: ended)
            }
            Composer(waiting: log?.status?.state == .waiting)
        }
        .background(Theme.chatBackground)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("chat")
    }

    @ViewBuilder
    private var messages: some View {
        if let log, log.loaded {
            ScrollViewReader { proxy in
                ScrollView {
                    LazyVStack(alignment: .leading, spacing: 18) {
                        if log.truncated {
                            Text("Earlier messages are not shown.")
                                .font(.caption)
                                .foregroundStyle(.secondary)
                                .frame(maxWidth: .infinity)
                        }
                        if log.entries.isEmpty {
                            ContentUnavailableView("No Messages Yet", systemImage: Icon.chat,
                                                   description: Text("Send a prompt to start."))
                        }
                        ForEach(log.entries) { entry in
                            EntryRow(entry: entry).id(entry.id)
                        }
                    }
                    .padding(.horizontal, 20)
                    .padding(.vertical, 16)
                    // A readable line length, like the agents' own apps.
                    .frame(maxWidth: 720)
                    .frame(maxWidth: .infinity)
                }
                .defaultScrollAnchor(.bottom)
                .scrollDismissesKeyboard(.interactively)
                .onChange(of: log.entries.last) { _, last in
                    guard let last else { return }
                    withAnimation(.easeOut(duration: 0.2)) { proxy.scrollTo(last.id, anchor: .bottom) }
                }
            }
        } else {
            ProgressView("Loading the conversation…")
                .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
    }
}

struct EntryRow: View {
    let entry: ChatEntry

    var body: some View {
        switch entry.kind {
        case .user:
            HStack {
                Spacer(minLength: 60)
                Text(entry.text)
                    .textSelection(.enabled)
                    .lineSpacing(3)
                    .padding(.horizontal, 16)
                    .padding(.vertical, 10)
                    .background(Theme.userBubble, in: RoundedRectangle(cornerRadius: 20, style: .continuous))
            }
        case .assistant:
            Markdown(entry.text)
                .markdownTextStyle {
                    FontSize(17)
                }
                .markdownBlockStyle(\.paragraph) { configuration in
                    configuration.label
                        .relativeLineSpacing(.em(0.3))
                        .markdownMargin(top: 0, bottom: 12)
                }
                .markdownBlockStyle(\.codeBlock) { configuration in
                    ScrollView(.horizontal, showsIndicators: false) {
                        configuration.label
                            .markdownTextStyle {
                                FontFamilyVariant(.monospaced)
                                FontSize(.em(0.85))
                            }
                            .padding(12)
                    }
                    .background(Theme.toolCard, in: RoundedRectangle(cornerRadius: 10, style: .continuous))
                    .markdownMargin(top: 4, bottom: 12)
                }
                .markdownTextStyle(\.code) {
                    FontFamilyVariant(.monospaced)
                    FontSize(.em(0.9))
                }
                .textSelection(.enabled)
                .frame(maxWidth: .infinity, alignment: .leading)
        case .tool:
            if let tool = entry.tool { ToolRow(tool: tool) }
        case .notice:
            Text(entry.text)
                .font(.caption)
                .foregroundStyle(.secondary)
                .frame(maxWidth: .infinity)
        }
    }
}

/// A tool call: its name and what it worked on; the result unfolds.
struct ToolRow: View {
    let tool: ChatTool
    @State private var open = false

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Button {
                withAnimation(.snappy) { open.toggle() }
            } label: {
                HStack(spacing: 8) {
                    Image(systemName: Self.icon(tool.name))
                        .foregroundStyle(tool.error ? Theme.failure : Color.secondary)
                        .frame(width: 18)
                    Text(tool.name).font(.callout.weight(.semibold))
                    Text(tool.summary)
                        .font(.callout.monospaced())
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                        .truncationMode(.middle)
                    Spacer(minLength: 0)
                    if tool.result == nil {
                        ProgressView().controlSize(.mini)
                    } else {
                        Image(systemName: "chevron.right")
                            .font(.caption.weight(.semibold))
                            .foregroundStyle(.tertiary)
                            .rotationEffect(.degrees(open ? 90 : 0))
                    }
                }
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .disabled(tool.result == nil)
            if open, let result = tool.result {
                Text(result.isEmpty ? "(no output)" : result)
                    .font(.caption.monospaced())
                    .foregroundStyle(tool.error ? Theme.failure : Color.secondary)
                    .textSelection(.enabled)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 8)
        .background(Theme.toolCard, in: RoundedRectangle(cornerRadius: 12, style: .continuous))
    }

    static func icon(_ name: String) -> String {
        switch name {
        case "Bash": "terminal"
        case "Read": "doc.text"
        case "Edit", "MultiEdit", "Write", "NotebookEdit": "pencil"
        case "Grep", "Glob": "magnifyingglass"
        case "WebFetch", "WebSearch": "globe"
        case "Task", "Agent": "person.2"
        case "TodoWrite": "checklist"
        default: "wrench.and.screwdriver"
        }
    }
}

/// The agent waits on something its adapter cannot name: only its terminal shows it.
struct WaitingBanner: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        HStack(spacing: 10) {
            Label("Waiting for your answer", systemImage: Icon.waiting)
                .font(.callout.weight(.medium))
                .foregroundStyle(Theme.attention)
            Spacer()
            ForEach(["1", "2", "3"], id: \.self) { key in
                QuickKey(title: key, keys: [key])
            }
            QuickKey(title: "Esc", keys: ["Escape"])
            Button("Show Raw") { model.setPretty(false) }
                .buttonStyle(.bordered)
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 8)
        .background(Theme.attention.opacity(0.12))
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("waiting")
    }
}

struct EndedBanner: View {
    @Environment(AppModel.self) private var model
    let reason: String

    var body: some View {
        HStack {
            Label("Stopped following: \(reason)", systemImage: "exclamationmark.circle")
                .font(.callout)
                .foregroundStyle(.secondary)
            Spacer()
            Button("Retry") { Task { await model.retryChat() } }
            Button("Show Raw") { model.setPretty(false) }
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 8)
    }
}

struct QuickKey: View {
    @Environment(AppModel.self) private var model
    let title: String
    let keys: [String]

    var body: some View {
        Button(title) { model.session.send(keys: keys) }
            .font(.footnote.monospaced())
            .buttonStyle(.bordered)
            .buttonBorderShape(.capsule)
            .controlSize(.small)
            .tint(.secondary)
            .accessibilityIdentifier("quick-\(keys.joined())")
    }
}

/// Return sends; Shift+Return starts a new line.
struct Composer: View {
    @Environment(AppModel.self) private var model
    let waiting: Bool
    @State private var text = ""
    @FocusState private var focused: Bool

    var body: some View {
        // One rounded box: the text, then the keys and Send, as in the agents' own apps.
        VStack(alignment: .leading, spacing: 10) {
            TextField("Message the agent", text: $text, axis: .vertical)
                .lineLimit(1...8)
                // Prompts carry slash commands, paths and code that a phone keyboard would "fix".
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
                .focused($focused)
                .onKeyPress(.return, phases: .down) { press in
                    if press.modifiers.contains(.shift) { return .ignored }
                    send()
                    return .handled
                }
                .accessibilityIdentifier("composer")
            HStack(spacing: 6) {
                ScrollView(.horizontal, showsIndicators: false) {
                    HStack(spacing: 6) {
                        QuickKey(title: "Esc", keys: ["Escape"])
                        QuickKey(title: "⏎", keys: ["Enter"])
                        QuickKey(title: "⇧Tab", keys: ["BTab"])
                        QuickKey(title: "↑", keys: ["Up"])
                        QuickKey(title: "↓", keys: ["Down"])
                        QuickKey(title: "^C", keys: ["C-c"])
                    }
                }
                Button(action: send) {
                    Image(systemName: Icon.send)
                        .font(.system(size: 30))
                        .symbolRenderingMode(.hierarchical)
                }
                .disabled(!canSend)
                .accessibilityLabel("Send")
                .accessibilityIdentifier("send")
            }
        }
        .padding(.horizontal, 14)
        .padding(.top, 12)
        .padding(.bottom, 8)
        .background(Theme.composer, in: RoundedRectangle(cornerRadius: 22, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: 22, style: .continuous).strokeBorder(.separator, lineWidth: 0.5))
        .shadow(color: .black.opacity(0.06), radius: 8, y: 2)
        .frame(maxWidth: 720)
        .padding(.horizontal, 16)
        .padding(.bottom, 10)
        .frame(maxWidth: .infinity)
        .onAppear { focused = KeyInputView.focusesByItself }
        .onChange(of: model.focusRequest) { if KeyInputView.focusesByItself { focused = true } }
    }

    /// While the agent waits on a prompt, Enter would answer it: send nothing then.
    private var canSend: Bool {
        !waiting && !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }

    private func send() {
        guard canSend else { return }
        model.sendPrompt(text)
        text = ""
    }
}
