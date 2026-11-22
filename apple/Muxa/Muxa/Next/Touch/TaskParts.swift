import MuxaV2
import SwiftUI

/// What a Task screen shows, after what the terminal can offer.
extension NextNavigation {
    func view(for t: MuxaV2.Terminal) -> TaskView {
        switch taskView {
        case .chat where !t.isTaskAgent, .reader where t.taskDir == nil: .terminal
        case let v: v
        }
    }
}

/// Chat, Terminal and Reader as segments, in the given order.
struct TaskSegments: View {
    @Environment(NextNavigation.self) private var nav
    let terminal: MuxaV2.Terminal
    var order: [NextNavigation.TaskView] = [.chat, .terminal, .reader]

    var body: some View {
        let picked = Binding(get: { nav.view(for: terminal) }, set: { nav.taskView = $0 })
        SegmentBar(segments: order.map(segment), selection: picked)
    }

    private func segment(_ v: NextNavigation.TaskView) -> Segment<NextNavigation.TaskView> {
        switch v {
        case .chat: Segment(value: v, title: "Chat", id: "focus.chat", enabled: terminal.isTaskAgent, key: "c")
        case .terminal: Segment(value: v, title: "Terminal", id: "focus.terminal")
        case .reader: Segment(value: v, title: "Reader", id: "focus.reader", enabled: terminal.taskDir != nil, key: "r")
        }
    }
}

/// The Task Loop's button: a symbol, and the failed checks while a loop runs.
struct LoopButton: View {
    @Environment(NextModel.self) private var model
    let terminal: MuxaV2.Terminal
    var open: () -> Void

    var body: some View {
        let lp = model.loops[terminal.id]
        Button(action: open) {
            HStack(spacing: 3) {
                Image(systemName: "arrow.triangle.2.circlepath")
                if let lp, lp.isActive {
                    Text("\(lp.attempts)/\(lp.maxAttempts)").font(.system(size: 13, weight: .medium)).monospacedDigit()
                }
            }
        }
        .buttonStyle(BareIconStyle(color: Palette.textSecondary))
        .disabled(!terminal.isTaskAgent)
        .accessibilityLabel(LoopText.title(lp))
        .accessibilityIdentifier("focus.loop")
    }
}

/// The phone's keys below the terminal (PhoneTab mock). Ctrl is on the
/// keyboard's own bar, where it can wait for the next key.
struct KeyRow: View {
    @Environment(NextModel.self) private var model
    let terminal: String

    private static let keys: [(text: String, name: String, label: String)] = [
        ("esc", "Escape", "Escape"), ("tab", "Tab", "Tab"), ("⇧tab", "BTab", "Shift Tab"), ("^C", "C-c", "Control C"),
        ("←", "Left", "Left"), ("↓", "Down", "Down"), ("↑", "Up", "Up"), ("→", "Right", "Right"),
    ]

    var body: some View {
        HStack(spacing: 5) {
            ForEach(Self.keys, id: \.name) { key in
                Button(key.text) { send(key.name) }
                    .font(.system(size: 13, weight: .medium, design: .monospaced))
                    .foregroundStyle(Palette.text)
                    .frame(maxWidth: .infinity, minHeight: minTouch)
                    .background(Color(hex: 0x1A1E25), in: RoundedRectangle(cornerRadius: 8))
                    .overlay(RoundedRectangle(cornerRadius: 8).stroke(Palette.borderStrong))
                    .buttonStyle(.plain)
                    .accessibilityLabel(key.label)
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityLabel("Keys")
    }

    private func send(_ name: String) {
        guard let bytes = KeyBytes.bytes(name) else { return }
        Task { await model.send(String(decoding: bytes, as: UTF8.self), to: terminal) }
    }
}

/// A line typed into the terminal, sent with Return at the end.
struct TerminalLine: View {
    @Environment(NextModel.self) private var model
    let terminal: String
    var prompt = "Type a line for the terminal"
    var focused: FocusState<Bool>.Binding
    @State private var text = ""

    var body: some View {
        HStack(spacing: 8) {
            TextField(prompt, text: $text)
                .focused(focused)
                .submitLabel(.send)
                .onSubmit(send)
                .padding(.horizontal, 4)
                .fieldWell(radius: minTouch / 2)
                .accessibilityIdentifier("focus.line")
            SendButton(disabled: text.isEmpty, action: send)
                .accessibilityIdentifier("focus.send")
        }
    }

    private func send() {
        guard !text.isEmpty else { return }
        let line = text
        text = ""
        Task { await model.send(line + "\r", to: terminal) }
    }
}

/// The choice a waiting agent offers, and its one Approve: the first option
/// that goes on, which the user picks (ADR 0009). Chat shows every option.
struct NeedsYouBar: View {
    @Environment(NextModel.self) private var model
    let terminal: MuxaV2.Terminal
    let prompt: ChatPrompt?

    var body: some View {
        if prompt != nil || model.needsYou.contains(terminal) {
            HStack(spacing: 10) {
                StateDot(color: TaskState.needsYou.color)
                Text("Needs you: \(prompt?.question ?? prompt?.title ?? "answer in the terminal")")
                    .font(.system(size: 14))
                    .foregroundStyle(Palette.text)
                    .lineLimit(2)
                Spacer(minLength: 0)
                if let option = prompt?.approve {
                    ApproveButton(option: option, terminal: terminal.id)
                }
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 6)
            .frame(minHeight: 52)
            .background(Palette.promptBack, in: RoundedRectangle(cornerRadius: 12))
            .overlay(RoundedRectangle(cornerRadius: 12).stroke(Palette.promptBorder))
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier("focus.needsyou")
        }
    }
}

/// Sends one checked option of the agent's menu. Its title is the option's own.
struct ApproveButton: View {
    @Environment(NextModel.self) private var model
    let option: ChatOption
    let terminal: String

    var body: some View {
        Button(option.label) { Task { await model.send(option.input, to: terminal) } }
            .buttonStyle(FilledButtonStyle(fill: TaskState.needsYou.color, text: Palette.onAmber))
            .accessibilityIdentifier("focus.approve")
    }
}

/// The terminal with Chat or the Reader over it. The terminal stays attached
/// under them, so its grid keeps its size and answers reach the agent.
struct TaskBody<Footer: View>: View {
    @Environment(NextNavigation.self) private var nav
    let terminal: MuxaV2.Terminal
    @Binding var chat: UInt32?
    /// The terminal ignores the keyboard unless a field of the footer has it.
    var keyboardInFooter = false
    @ViewBuilder var footer: Footer

    var body: some View {
        ZStack {
            VStack(spacing: 0) {
                TerminalPane(terminal: terminal)
                    .id(terminal.id)
                    .background(Palette.terminal)
                    .clipShape(RoundedRectangle(cornerRadius: 10))
                    .padding(.horizontal, 6)
                footer
            }
            .ignoresSafeArea(keyboardInFooter ? [] : .keyboard, edges: .bottom)
            switch nav.view(for: terminal) {
            case .chat: ChatView(terminal: terminal, chat: $chat)
            case .reader: ReaderPanel(terminal: terminal).id(terminal.id).background(Palette.ground)
            case .terminal: EmptyView()
            }
        }
    }
}
