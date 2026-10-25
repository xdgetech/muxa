import MuxaProtocol
import MuxaSession
import MuxaUI
import SwiftUI

struct PaneScreen: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        VStack(spacing: 0) {
            bar
            ZStack(alignment: .bottom) {
                // Fits above an on-screen keyboard, so every row stays visible.
                PaneHost(session: model.session)
                if let notice {
                    Text(notice)
                        .font(.callout)
                        .padding(.horizontal, 12)
                        .padding(.vertical, 6)
                        .background(.thinMaterial, in: Capsule())
                        .padding(.bottom, 12)
                }
            }
        }
        .background(.black)
    }

    private var notice: String? {
        if let gone = model.session.paneGoneReason { return "Pane closed: \(gone). Cmd+K to pick another." }
        if model.session.watchedPane == nil { return "Cmd+K to pick a session." }
        return model.session.lastError
    }

    private var bar: some View {
        HStack(spacing: 12) {
            Button {
                model.showSwitcher = true
            } label: {
                HStack(spacing: 6) {
                    Text(model.currentSession?.name ?? "Sessions").font(.headline)
                    Image(systemName: "chevron.down").font(.caption)
                }
            }
            if let pane = model.currentPane { AgentBadge(pane: pane) }
            Spacer()
            if let pane = model.currentPane {
                Text("\(pane.cols)×\(pane.rows)").font(.caption.monospaced()).foregroundStyle(.secondary)
            }
            Picker("Pane", selection: toggleBinding) {
                Text("Agent").tag(Role.agent)
                Text("Terminal").tag(Role.shell)
            }
            .pickerStyle(.segmented)
            .frame(width: 180)
            .disabled(model.counterpart == nil)
            Button {
                Task { await model.disconnect() }
            } label: {
                Image(systemName: "xmark.circle")
            }
            .accessibilityLabel("Disconnect")
        }
        .padding(.horizontal)
        .padding(.vertical, 8)
        .background(.bar)
    }

    /// Agent when the shown pane is the agent side, else Terminal.
    private var toggleBinding: Binding<Role> {
        Binding(
            get: { isAgentSide ? .agent : .shell },
            set: { want in
                if want != (isAgentSide ? .agent : .shell) {
                    Task { await model.toggleAgentTerminal() }
                }
            })
    }

    private var isAgentSide: Bool {
        guard let pane = model.currentPane else { return true }
        switch pane.role {
        case .agent: return true
        case .shell: return false
        case .hidden, .unknown: return !pane.shellForeground
        }
    }
}

/// The terminal view with a key input view over it, wired to the session.
struct PaneHost: UIViewRepresentable {
    let session: PaneSession

    func makeUIView(context: Context) -> PaneContainer {
        let view = PaneContainer()
        session.renderer = view.terminal
        view.input.onOutput = { [session] output in
            switch output {
            case .keys(let keys): session.send(keys: keys)
            case .text(let text): session.send(text: text)
            case .none: break
            }
        }
        view.input.onPaste = { [session] text in session.paste(text) }
        return view
    }

    func updateUIView(_ view: PaneContainer, context: Context) {}
}

final class PaneContainer: UIView {
    let terminal = PaneTerminalView()
    let input = KeyInputView()

    init() {
        super.init(frame: .zero)
        backgroundColor = .black
        addSubview(terminal)
        addSubview(input)
        input.backgroundColor = .clear
        input.accessibilityIdentifier = "pane"
        input.screenText = { [terminal] in terminal.screenText }
        input.addGestureRecognizer(UITapGestureRecognizer(target: input, action: #selector(UIResponder.becomeFirstResponder)))
    }

    required init?(coder: NSCoder) { nil }

    override func layoutSubviews() {
        super.layoutSubviews()
        terminal.frame = bounds
        input.frame = bounds
    }

    override func didMoveToWindow() {
        super.didMoveToWindow()
        if window != nil { input.becomeFirstResponder() }
    }
}
