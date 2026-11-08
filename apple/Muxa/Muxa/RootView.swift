import MuxaProtocol
import MuxaSession
import SwiftUI

/// The sidebar's mode on the left, what it selects in the detail (ADR 0035). The
/// sidebar hides once a pane is open, so the pane gets the full width. On a phone
/// the two are a stack, and the selection is pushed.
struct RootView: View {
    @Environment(AppModel.self) private var model
    @Environment(\.phoneLayout) private var phoneLayout
    @State private var columns: NavigationSplitViewVisibility = .all
    /// Collapsed, `columns` is ignored; this picks the column instead.
    @State private var compactColumn: NavigationSplitViewColumn = .sidebar
    @State private var editing: HostDraft?

    var body: some View {
        @Bindable var model = model
        NavigationSplitView(columnVisibility: $columns, preferredCompactColumn: $compactColumn) {
            Sidebar(editing: $editing)
        } detail: {
            // Under the navigation bar, so it never covers the toolbar.
            DetailView(editing: $editing)
                .overlay(alignment: .top) {
                    if let alert = model.alert {
                        AlertBanner(alert: alert) { Task { await model.openAlert() } }
                            .padding(.top, 8)
                            .transition(.move(edge: .top).combined(with: .opacity))
                    }
                }
                .animation(.snappy, value: model.alert)
        }
        .navigationSplitViewStyle(.balanced)
        .onAppear { model.phoneLayout = phoneLayout }
        .onChange(of: phoneLayout) { _, value in model.phoneLayout = value }
        .onChange(of: model.session.sessions) { old, new in
            model.sessionsChanged(from: old, to: new)
        }
        .onChange(of: model.paneRequest) { columns = .detailOnly }
        .onChange(of: model.detailRequest) { compactColumn = .detail }
        .onChange(of: model.sidebarRequest) {
            columns = .all
            compactColumn = .sidebar
        }
        .onChange(of: model.sidebarToggle) {
            columns = columns == .detailOnly ? .all : .detailOnly
            if compactColumn == .detail { compactColumn = .sidebar }
        }
        .onChange(of: model.isConnected) { _, connected in
            if !connected, !model.suspended {
                model.clearDetail()
                columns = .all
                compactColumn = .sidebar
            }
        }
        .alert("Can't Update the Inbox", isPresented: Binding(get: { model.laterFailure != nil }, set: { if !$0 { model.laterFailure = nil } })) {
            Button("OK", role: .cancel) {}
        } message: {
            Text(model.laterFailure ?? "")
        }
        .sheet(isPresented: $model.showSettings, onDismiss: model.refocusPane) {
            SettingsScreen()
        }
        .sheet(item: $editing, onDismiss: model.refocusPane) { draft in
            HostEditor(draft: draft)
        }
        .sheet(item: $model.untrustedKey, onDismiss: model.refocusPane) { key in
            // Only Trust or Cancel closes it, so a held setup password is always cleared.
            TrustHostSheet(key: key).interactiveDismissDisabled()
        }
        .paneSheets()
        .sheet(isPresented: $model.showSwitcher, onDismiss: model.switcherClosed) {
            SessionSwitcher()
        }
        .sheet(
            isPresented: Binding(get: { model.newTask != nil }, set: { if !$0 { model.newTask = nil } }),
            onDismiss: model.refocusPane
        ) {
            if let draft = model.newTask { NewTaskSheet(draft: draft) }
        }
        .sheet(
            isPresented: Binding(get: { model.newRecord != nil }, set: { if !$0 { model.newRecord = nil } }),
            onDismiss: { Task { await model.recordSheetClosed() } }
        ) {
            if let draft = model.newRecord {
                CreateTaskSheet(draft: draft) { model.recordCreated($0) }
            }
        }
        .confirmationDialog(
            "Close \(model.closingTask?.name ?? "Task")?",
            isPresented: Binding(get: { model.closingTask != nil }, set: { if !$0 { model.closingTask = nil } }),
            titleVisibility: .visible,
            presenting: model.closingTask
        ) { task in
            Button("Close") { Task { await model.close(task, mode: .close) } }
            Button("Close and Keep Worktree") { Task { await model.close(task, mode: .keep) } }
            Button("Remove Worktree Anyway", role: .destructive) { Task { await model.close(task, mode: .force) } }
            Button("Cancel", role: .cancel) {}
        } message: { _ in
            Text("Close removes the worktree only when the branch is merged. Uncommitted changes are never removed, and the branch is kept.")
        }
        .closeAskSession()
        .alert(
            "Can't Ask",
            isPresented: Binding(get: { model.askError != nil }, set: { if !$0 { model.askError = nil } })
        ) {
            Button("OK", role: .cancel) {}
        } message: {
            Text(model.askError ?? "")
        }
        .alert(
            "Can't Close the Task",
            isPresented: Binding(get: { model.closeError != nil }, set: { if !$0 { model.closeError = nil } })
        ) {
            Button("OK", role: .cancel) { model.refocusPane() }
        } message: {
            Text(model.closeError ?? "")
        }
        .fullScreenCover(isPresented: $model.showReader, onDismiss: model.readerClosed) {
            ReaderScreen()
        }
    }
}

/// What the sidebar selects, or the pane; before a connection, its state.
struct DetailView: View {
    @Environment(AppModel.self) private var model
    @Binding var editing: HostDraft?

    var body: some View {
        if !model.isConnected {
            ConnectionState(editing: $editing)
        } else if model.detail == .pane, model.detailPath.isEmpty {
            // The pane as it always was: the detail itself, not inside a stack.
            pane
        } else {
            @Bindable var model = model
            NavigationStack(path: $model.detailPath) {
                selection
                    .navigationDestination(for: DetailRoute.self) { route in
                        switch route {
                        case .routine(let workspace, let name): RoutineDetail(workspace: workspace, name: name)
                        case .pane: pane
                        }
                    }
            }
        }
    }

    @ViewBuilder private var pane: some View {
        if model.session.watchedPane != nil {
            // The pane can be watched before its view exists, e.g. from an item: the new
            // view asks for the screen again, so it never starts empty.
            PaneScreen().onAppear { Task { await model.paneAppeared() } }
        } else {
            SessionsEmptyDetail()
        }
    }

    @ViewBuilder private var selection: some View {
        switch model.detail {
        case .item(let id): ItemDetail(id: id)
        case .task(let id): TaskDetail(id: id)
        case .routines: RoutinesScreen()
        case .pane: pane
        case nil:
            switch model.mode {
            case .inbox: InboxEmptyDetail()
            case .tasks: TasksEmptyDetail()
            case .sessions: SessionsEmptyDetail()
            }
        }
    }
}

/// Connecting, adding the key, or why the connection failed with what to do next;
/// on first launch, how to start.
struct ConnectionState: View {
    @Environment(AppModel.self) private var model
    @Binding var editing: HostDraft?

    var body: some View {
        if model.addingKey, let host = model.currentHost {
            ProgressView("Adding this device's key to \(host.title)…")
        } else if model.connecting, let host = model.currentHost {
            ProgressView("Connecting to \(host.title)…")
        } else if let error = model.connectError ?? reason, let host = model.currentHost {
            ContentUnavailableView {
                Label("Can't Connect to \(host.title)", systemImage: Icon.warning)
            } description: {
                Text(error)
            } actions: {
                switch model.connectFix {
                case .addKey:
                    Button("Set Up with Password") { editing = HostDraft(host) }
                        .buttonStyle(.borderedProminent)
                    Button("Try Again") { Task { await model.connect(to: host.id) } }
                case .installBridge:
                    Button("Try Again") { Task { await model.connect(to: host.id) } }
                        .buttonStyle(.borderedProminent)
                    Link("How to Install the Bridge", destination: Guide.hostSetup)
                    Button("Edit Host") { editing = HostDraft(host) }
                case nil:
                    Button("Try Again") { Task { await model.connect(to: host.id) } }
                        .buttonStyle(.borderedProminent)
                    Button("Edit Host") { editing = HostDraft(host) }
                }
            }
        } else if model.hosts.hosts.isEmpty {
            ContentUnavailableView {
                Label("Add a Host", systemImage: Icon.host)
            } description: {
                Text("Muxa connects over SSH to the Mac or Linux machine where your tmux sessions run. Install muxa-bridge there first.")
            } actions: {
                Button("Add Host") { editing = HostDraft() }.buttonStyle(.borderedProminent)
                Button("Try the Demo") { Task { await model.startDemo() } }
                    .accessibilityIdentifier("try-demo")
                Link("How to Set Up the Host", destination: Guide.hostSetup)
            }
        } else {
            ContentUnavailableView("Not Connected", systemImage: "bolt.horizontal",
                                   description: Text("Connect to a host from the sidebar, or pick one in Settings."))
        }
    }

    /// Why the last connection ended, unless the user left on purpose.
    private var reason: String? {
        if case .disconnected(let reason) = model.session.phase, reason != "disconnected" { return reason }
        return nil
    }
}

/// Setup steps for the host live in the README, next to the bridge they install.
private enum Guide {
    // A literal that always parses.
    static let hostSetup = URL(string: "https://github.com/xdgetech/muxa#setup")!
}

/// "feature-auth: needs your input". Tapping it opens that session.
struct AlertBanner: View {
    let alert: AgentAlert
    let open: () -> Void

    var body: some View {
        Button(action: open) {
            Label {
                Text("\(alert.sessionName): \(alert.message)")
                    .lineLimit(1)
            } icon: {
                Image(systemName: icon).foregroundStyle(color)
            }
            .font(.callout.weight(.medium))
            .padding(.horizontal, 16)
            .padding(.vertical, 10)
            .background(.regularMaterial, in: Capsule())
            .shadow(color: .black.opacity(0.15), radius: 8, y: 2)
        }
        .buttonStyle(.plain)
        .accessibilityIdentifier("agent-alert")
    }

    private var icon: String {
        switch alert.kind {
        case .waiting: Icon.waiting
        case .done: "checkmark.circle.fill"
        case .failed: Icon.warning
        }
    }

    private var color: Color {
        switch alert.kind {
        case .waiting: Theme.state(.waiting)
        case .done: Theme.state(.done)
        case .failed: Theme.state(.error)
        }
    }
}
