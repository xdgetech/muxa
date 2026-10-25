import MuxaProtocol
import MuxaSession
import MuxaUI
import SwiftUI

struct PaneScreen: View {
    @Environment(AppModel.self) private var model
    /// Nil: fit the width in a wide view, the whole pane otherwise.
    @State private var fit: PaneTerminalView.FitMode?

    var body: some View {
        ZStack(alignment: .bottom) {
            // Fits above an on-screen keyboard, so every row stays visible. It stays
            // under the Pretty view so switching back is instant.
            PaneHost(session: model.session, fit: fit, focus: model.showsPretty ? nil : model.focusRequest)
            if model.showsPretty {
                ChatView(log: model.chat)
            }
            if let notice {
                Text(notice)
                    .font(.callout)
                    .padding(.horizontal, 12)
                    .padding(.vertical, 6)
                    .background(.thinMaterial, in: Capsule())
                    .padding(.bottom, 12)
            }
        }
        .background(.black)
        .navigationTitle(model.currentSession?.name ?? "Sessions")
        .paneSubtitle(subtitle)
        .navigationBarTitleDisplayMode(.inline)
        // The bar sits on the black pane, so it uses dark text and controls.
        .toolbarColorScheme(.dark, for: .navigationBar)
        .toolbarBackground(.black, for: .navigationBar)
        .toolbarBackground(.visible, for: .navigationBar)
        .toolbarTitleMenu {
            ForEach(model.visibleSessions) { s in
                Button(s.name) { Task { await model.open(s) } }
            }
        }
        .toolbar { toolbar }
        // Leaving Pretty, by the toggle or by opening another pane, returns the keyboard.
        .onChange(of: model.showsPretty) { _, pretty in
            if !pretty { model.refocusPane() }
        }
    }

    /// The adapter's state in Pretty, the pane's tags otherwise.
    private var subtitle: String {
        guard let pane = model.currentPane else { return "" }
        if model.showsPretty, let status = model.chat?.status {
            var tagged = pane
            tagged.agent = status
            return AgentBadge.summary(tagged)
        }
        return AgentBadge.summary(pane)
    }

    private var notice: String? {
        if let gone = model.session.paneGoneReason { return "Pane closed: \(gone). Cmd+K to pick another." }
        return model.connectError
    }

    @ToolbarContentBuilder
    private var toolbar: some ToolbarContent {
        ToolbarItemGroup(placement: .topBarTrailing) {
            if model.canShowPretty {
                Picker("View", selection: prettyBinding) {
                    Label("Pretty", systemImage: "text.bubble").tag(true)
                    Label("Raw", systemImage: "terminal").tag(false)
                }
                .pickerStyle(.segmented)
                .fixedSize()
                .accessibilityIdentifier("view-mode")
            }
            Picker("Pane", selection: toggleBinding) {
                Text("Agent").tag(Role.agent)
                Text("Terminal").tag(Role.shell)
            }
            .pickerStyle(.segmented)
            .fixedSize()
            .disabled(model.counterpart == nil)
            Menu {
                Picker("Fit", selection: $fit) {
                    Label("Automatic", systemImage: "rectangle.dashed").tag(PaneTerminalView.FitMode?.none)
                    Label("Fit Width", systemImage: "arrow.left.and.right").tag(PaneTerminalView.FitMode?.some(.width))
                    Label("Fit Whole Pane", systemImage: "arrow.up.left.and.arrow.down.right")
                        .tag(PaneTerminalView.FitMode?.some(.whole))
                }
            } label: {
                Label("Fit", systemImage: "textformat.size")
            }
            Button("Sessions", systemImage: "rectangle.stack") { model.showSwitcher = true }
            Button("Disconnect", systemImage: "xmark.circle") { Task { await model.disconnect() } }
        }
    }

    private var prettyBinding: Binding<Bool> {
        Binding(get: { model.showsPretty }, set: { model.setPretty($0) })
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
        model.currentPane?.isAgentSide ?? true
    }
}

extension View {
    /// The agent state and pane size under the title, where the system supports it.
    @ViewBuilder
    func paneSubtitle(_ text: String) -> some View {
        if #available(iOS 26, *) {
            navigationSubtitle(text)
        } else {
            self
        }
    }
}

/// The terminal view with a key input view over it, wired to the session.
struct PaneHost: UIViewRepresentable {
    let session: PaneSession
    /// Nil picks the mode from the view's shape.
    var fit: PaneTerminalView.FitMode?
    /// A new value takes the keyboard back, for example after a sheet closed.
    /// Nil while another view (Pretty) has the keyboard.
    var focus: Int?

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

    func updateUIView(_ view: PaneContainer, context: Context) {
        view.fitOverride = fit
        if let focus, view.focusRequest != focus {
            view.focusRequest = focus
            // After SwiftUI has removed whatever held the keyboard (a sheet, the composer).
            DispatchQueue.main.async { view.input.becomeFirstResponder() }
        }
    }
}

/// Holds the pane in a vertical scroll view. In `.width` mode the grid is taller
/// than the view; the scroll view follows the cursor unless the user is reading.
final class PaneContainer: UIView, UIScrollViewDelegate {
    let terminal = PaneTerminalView()
    let input = KeyInputView()
    private let scroll = UIScrollView()
    private var cursorWasVisible = true
    var focusRequest = 0

    var fitOverride: PaneTerminalView.FitMode? {
        didSet { if fitOverride != oldValue { setNeedsLayout() } }
    }

    init() {
        super.init(frame: .zero)
        backgroundColor = .black
        scroll.backgroundColor = .black
        scroll.alwaysBounceVertical = false
        scroll.showsHorizontalScrollIndicator = false
        scroll.indicatorStyle = .white
        scroll.contentInsetAdjustmentBehavior = .never
        scroll.keyboardDismissMode = .none
        scroll.delegate = self
        addSubview(scroll)
        // The input view lies over the grid inside the scroll view, so drags still scroll.
        scroll.addSubview(terminal)
        scroll.addSubview(input)
        input.backgroundColor = .clear
        input.accessibilityIdentifier = "pane"
        input.screenText = { [terminal] in terminal.screenText }
        input.addGestureRecognizer(UITapGestureRecognizer(target: input, action: #selector(UIResponder.becomeFirstResponder)))
        terminal.onUpdate = { [weak self] reset in self?.contentChanged(reset: reset) }
        terminal.onGridChange = { [weak self] in self?.layoutContent() }
    }

    required init?(coder: NSCoder) { nil }

    /// Wide views fit the width; the rest fit the whole pane.
    private var mode: PaneTerminalView.FitMode {
        fitOverride ?? (bounds.width > bounds.height ? .width : .whole)
    }

    override func layoutSubviews() {
        super.layoutSubviews()
        layoutContent()
    }

    /// Sizes the scroll content to the grid in the current mode.
    private func layoutContent() {
        scroll.frame = bounds
        let mode = mode
        terminal.fitMode = mode
        let height = mode == .width ? max(terminal.gridHeight(forWidth: bounds.width), bounds.height) : bounds.height
        let content = CGRect(x: 0, y: 0, width: bounds.width, height: height)
        if terminal.frame != content {
            terminal.frame = content
            input.frame = content
            scroll.contentSize = content.size
            terminal.layoutIfNeeded()
            followCursor()
        }
    }

    override func didMoveToWindow() {
        super.didMoveToWindow()
        if window != nil { input.becomeFirstResponder() }
    }

    private func contentChanged(reset: Bool) {
        let wasVisible = reset || cursorWasVisible
        // SwiftTerm moves its caret after the feed is processed.
        DispatchQueue.main.async { [weak self] in
            guard let self else { return }
            if wasVisible { self.followCursor() }
            self.cursorWasVisible = self.cursorIsVisible
        }
    }

    private var cursorIsVisible: Bool {
        scroll.bounds.contains(terminal.cursorFrame)
    }

    /// Brings the cursor row into view, but never while the user is scrolling.
    private func followCursor() {
        guard !scroll.isDragging, !scroll.isDecelerating, scroll.contentSize.height > scroll.bounds.height else { return }
        let caret = terminal.cursorFrame
        guard caret.height > 0 else { return }
        scroll.scrollRectToVisible(CGRect(x: 0, y: caret.minY, width: 1, height: caret.height), animated: false)
        cursorWasVisible = true
    }

    func scrollViewDidEndDragging(_ scrollView: UIScrollView, willDecelerate decelerate: Bool) {
        if !decelerate { cursorWasVisible = cursorIsVisible }
    }

    func scrollViewDidEndDecelerating(_ scrollView: UIScrollView) {
        cursorWasVisible = cursorIsVisible
    }
}
