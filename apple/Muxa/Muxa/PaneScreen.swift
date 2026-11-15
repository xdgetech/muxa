import MuxaProtocol
import MuxaSession
import MuxaUI
import SwiftUI

struct PaneScreen: View {
    @Environment(AppModel.self) private var model
    @Environment(\.phoneLayout) private var phoneLayout
    @Environment(\.colorScheme) private var colorScheme

    var body: some View {
        ZStack(alignment: .bottom) {
            // Fits above an on-screen keyboard, so every row stays visible. It stays
            // under the Pretty view so switching back is instant.
            PaneHost(
                session: model.session, fit: model.paneFit, focus: model.showsPretty ? nil : model.focusRequest,
                snapshot: model.paneSnapshot,
                onNeedSnapshot: model.canShowHistory ? { model.refreshPaneSnapshot() } : nil,
                onPinch: { model.paneFit = $0 }, onScrollBack: { model.showHistory() },
                showsScrollback: model.history != nil, onLive: { model.closeHistory() })
            if model.history != nil {
                ScrollbackLayer()
            } else if model.showsPretty {
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
        // A phone's bar has room for the title and More only: iOS folds whatever does not fit
        // into a menu of its own. The picker and Documents get a row of their own, as on Android.
        // Not in the title's place: the title menu takes its taps.
        .safeAreaInset(edge: .top, spacing: 0) {
            if phoneLayout, model.paneViews.count > 1 || model.canReadDocs {
                ZStack(alignment: .trailing) {
                    if model.paneViews.count > 1 { viewPicker.frame(maxWidth: .infinity) }
                    if model.canReadDocs {
                        documentsButton.frame(width: 44, height: 32).contentShape(Rectangle()).padding(.trailing, 8)
                    }
                }
                .frame(maxWidth: .infinity, minHeight: 32)
                .padding(.vertical, 6)
                .background(model.showsPretty ? Theme.chatBackground : Theme.terminal)
                .environment(\.colorScheme, model.showsPretty ? colorScheme : .dark)
            }
        }
        .background(Theme.terminal)
        .navigationTitle(title)
        .paneSubtitle(phoneLayout ? "" : subtitle)
        .navigationBarTitleDisplayMode(.inline)
        // Over the black terminal the bar is dark; over the chat it matches the chat.
        .toolbarColorScheme(model.showsPretty ? nil : .dark, for: .navigationBar)
        .toolbarBackground(model.showsPretty ? Theme.chatBackground : Theme.terminal, for: .navigationBar)
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

    private var title: String { model.currentSession?.name ?? "Sessions" }

    @ToolbarContentBuilder
    private var toolbar: some ToolbarContent {
        // iOS keeps a long title whole and folds More into a menu of its own, so a phone's
        // title is narrower than its bar: it fits More beside it on the narrowest iPhone.
        if phoneLayout {
            ToolbarItem(placement: .principal) {
                VStack(spacing: 0) {
                    Text(title).font(.headline).truncationMode(.middle)
                    if !subtitle.isEmpty {
                        Text(subtitle).font(.caption2).foregroundStyle(.secondary)
                    }
                }
                .lineLimit(1)
                .frame(maxWidth: Self.phoneTitleWidth)
                .accessibilityElement(children: .combine)
            }
        }
        ToolbarItemGroup(placement: .topBarTrailing) {
            if !phoneLayout {
                viewPicker
                if model.canReadDocs { documentsButton }
            }
            Menu {
                Picker(selection: fitBinding) {
                    Label("Automatic", systemImage: "rectangle.dashed").tag(PaneFit?.none)
                    Label("Readable", systemImage: "textformat.size").tag(PaneFit?.some(.readable))
                    Label("Fit Width", systemImage: "arrow.left.and.right").tag(PaneFit?.some(.width))
                    Label("Fit Whole Pane", systemImage: "arrow.up.left.and.arrow.down.right")
                        .tag(PaneFit?.some(.whole))
                } label: {
                    Label("Terminal Size", systemImage: Icon.fit)
                }
                .pickerStyle(.menu)
                // Also on a phone: its list holds manual sessions only (ADR 0027).
                Button("Sessions…", systemImage: Icon.session) { model.showSwitcher = true }
                if model.canShowHistory {
                    Button("Scrollback", systemImage: Icon.history) { model.showHistory() }
                        .accessibilityIdentifier("menu-history")
                }
                if let task = model.paneTask {
                    Button("Task Info", systemImage: Icon.tasks) { model.taskInfo = task.id }
                        .accessibilityIdentifier("menu-task-info")
                }
                if model.canCreateTasks {
                    Button("New Task…", systemImage: Icon.newTask) { model.startNewTask() }
                }
                if let current = model.currentSession, model.canClose(current) {
                    Button("Close Task…", systemImage: Icon.closeTask) { model.closingTask = current }
                }
                if let current = model.currentSession, model.canCloseAsk(current) {
                    Button("Close Session…", systemImage: Icon.closeTask) { model.closingAsk = current }
                }
                Divider()
                Button("Disconnect", systemImage: Icon.disconnect, role: .destructive) {
                    Task { await model.disconnect() }
                }
            } label: {
                Label("More", systemImage: Icon.more)
            }
        }
    }

    /// 375 pt (iPhone SE) less the margins, Back and More.
    private static let phoneTitleWidth: CGFloat = 200

    private var documentsButton: some View {
        Button { model.openReader() } label: {
            // A dot when the repo has uncommitted changes to look at.
            Image(systemName: Icon.document)
                .overlay(alignment: .topTrailing) {
                    if model.changeCount > 0 {
                        Circle().fill(Theme.accent).frame(width: 7, height: 7).offset(x: 3, y: -2)
                    }
                }
        }
        .accessibilityLabel(model.changeCount > 0 ? "Documents, \(model.changeCount) changed files" : "Documents")
    }

    private var viewPicker: some View {
        Picker("View", selection: viewBinding) {
            ForEach(model.paneViews, id: \.self) { view in
                Text(Self.title(view)).tag(view)
            }
        }
        .pickerStyle(.segmented)
        .fixedSize()
        .accessibilityIdentifier("view-mode")
    }

    static func title(_ view: AppModel.PaneView) -> String {
        switch view {
        case .chat: "Chat"
        case .agent: "Agent"
        case .shell: "Shell"
        }
    }

    private var fitBinding: Binding<PaneFit?> {
        Binding(get: { model.paneFit }, set: { model.paneFit = $0 })
    }

    private var viewBinding: Binding<AppModel.PaneView> {
        Binding(get: { model.paneView }, set: { view in Task { await model.show(view) } })
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
    /// Nil lets the view pick (`PaneFit.automatic`).
    var fit: PaneFit?
    /// A new value takes the keyboard back, for example after a sheet closed,
    /// when a hardware keyboard is connected.
    /// Nil while another view (Pretty) has the keyboard.
    var focus: Int?
    /// The pane's latest scrollback, shown in place when the user reads back.
    var snapshot: PaneHistory?
    /// Asks for a newer snapshot; nil when the host cannot send one.
    var onNeedSnapshot: (() -> Void)?
    /// The user pinched to another fit.
    var onPinch: (PaneFit) -> Void = { _ in }
    /// No snapshot to read in place: show the scrollback once it loads.
    var onScrollBack: () -> Void = {}
    /// The scrollback from More, or from `onScrollBack`, shows in the pane's place.
    var showsScrollback = false
    /// Leaves that scrollback.
    var onLive: () -> Void = {}

    func makeUIView(context: Context) -> PaneContainer {
        let view = PaneContainer()
        session.renderer = view.terminal
        view.terminal.onScreenLost = { [session] in Task { await session.resync() } }
        view.input.onOutput = { [session, weak view] output in
            if let view, view.isReadingBack {
                view.showLive()
                // Escape only leaves the scrollback: in an agent it would stop the turn.
                if output == Self.escape { return }
            }
            switch output {
            case .keys(let keys): session.send(keys: keys)
            case .text(let text): session.send(text: text)
            case .none: break
            }
        }
        view.input.onPaste = { [session, weak view] text in
            if let view, view.isReadingBack { view.showLive() }
            session.paste(text)
        }
        view.onViewport = { [session] grid in session.setViewport(grid) }
        return view
    }

    private static let escape = KeyMap.map(special: .escape, characters: "", modifiers: [])

    func updateUIView(_ view: PaneContainer, context: Context) {
        view.fitOverride = fit
        view.onPinch = onPinch
        view.onNeedSnapshot = onNeedSnapshot
        view.snapshot = snapshot?.pane == session.watchedPane ? snapshot : nil
        view.onScrollBack = onScrollBack
        view.showsScrollback = showsScrollback
        view.onLive = onLive
        if let focus, view.focusRequest != focus {
            view.focusRequest = focus
            // After SwiftUI has removed whatever held the keyboard (a sheet, the composer).
            DispatchQueue.main.async { view.input.focusIfKeyboard() }
        }
    }
}
