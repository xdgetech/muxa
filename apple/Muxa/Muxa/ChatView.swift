import MarkdownUI
import MuxaProtocol
import MuxaSession
import SwiftUI

/// The Pretty view of an agent pane: the conversation as messages and a composer.
/// Everything sent still goes to the same pane, as a paste and Enter.
struct ChatView: View {
    @Environment(AppModel.self) private var model
    let log: ChatLog?

    var body: some View {
        VStack(spacing: 0) {
            messages
            Divider()
            if log?.status?.state == .waiting {
                WaitingBanner()
            }
            if let ended = log?.ended {
                EndedBanner(reason: ended)
            }
            Composer(waiting: log?.status?.state == .waiting)
        }
        .background(Color(.systemBackground))
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("chat")
    }

    @ViewBuilder
    private var messages: some View {
        if let log, log.loaded {
            ScrollViewReader { proxy in
                ScrollView {
                    LazyVStack(alignment: .leading, spacing: 14) {
                        if log.truncated {
                            Text("Earlier messages are not shown.")
                                .font(.caption)
                                .foregroundStyle(.secondary)
                                .frame(maxWidth: .infinity)
                        }
                        if log.entries.isEmpty {
                            ContentUnavailableView("No Messages Yet", systemImage: "text.bubble",
                                                   description: Text("Send a prompt to start."))
                        }
                        ForEach(log.entries) { entry in
                            EntryRow(entry: entry).id(entry.id)
                        }
                    }
                    .padding(.horizontal, 16)
                    .padding(.vertical, 12)
                    .frame(maxWidth: 820)
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
                    .padding(.horizontal, 14)
                    .padding(.vertical, 10)
                    .background(Color.accentColor.opacity(0.22), in: RoundedRectangle(cornerRadius: 18, style: .continuous))
            }
        case .assistant:
            Markdown(entry.text)
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
                        .foregroundStyle(tool.error ? Color.red : Color.secondary)
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
                    .foregroundStyle(tool.error ? Color.red : Color.secondary)
                    .textSelection(.enabled)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 8)
        .background(Color(.secondarySystemBackground), in: RoundedRectangle(cornerRadius: 12, style: .continuous))
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

/// The agent asks something only its terminal shows, e.g. a permission prompt.
struct WaitingBanner: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        HStack(spacing: 10) {
            Label("Waiting for your answer", systemImage: "hand.raised.fill")
                .font(.callout.weight(.medium))
                .foregroundStyle(.orange)
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
        .background(Color.orange.opacity(0.12))
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
            .font(.callout.monospaced())
            .buttonStyle(.bordered)
            .buttonBorderShape(.capsule)
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
        VStack(spacing: 8) {
            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: 8) {
                    QuickKey(title: "Esc", keys: ["Escape"])
                    QuickKey(title: "⏎", keys: ["Enter"])
                    QuickKey(title: "⇧Tab", keys: ["BTab"])
                    QuickKey(title: "↑", keys: ["Up"])
                    QuickKey(title: "↓", keys: ["Down"])
                    QuickKey(title: "^C", keys: ["C-c"])
                }
                .padding(.horizontal, 16)
            }
            HStack(alignment: .bottom, spacing: 10) {
                TextField("Message the agent", text: $text, axis: .vertical)
                    .lineLimit(1...8)
                    .focused($focused)
                    .padding(.horizontal, 14)
                    .padding(.vertical, 10)
                    .background(Color(.secondarySystemBackground), in: RoundedRectangle(cornerRadius: 20, style: .continuous))
                    .onKeyPress(.return, phases: .down) { press in
                        if press.modifiers.contains(.shift) { return .ignored }
                        send()
                        return .handled
                    }
                    .accessibilityIdentifier("composer")
                Button(action: send) {
                    Image(systemName: "arrow.up.circle.fill")
                        .font(.system(size: 34))
                }
                .disabled(!canSend)
                .accessibilityLabel("Send")
                .accessibilityIdentifier("send")
            }
            .padding(.horizontal, 16)
        }
        .padding(.vertical, 10)
        .onAppear { focused = true }
        .onChange(of: model.focusRequest) { focused = true }
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
