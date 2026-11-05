import MuxaProtocol
import MuxaSession
import SwiftUI

/// The sidebar shows one mode at a time (ADR 0035): Inbox, Tasks or Sessions, picked
/// under the title. Hosts live in Settings; a status row shows the connection.
struct Sidebar: View {
    @Environment(AppModel.self) private var model
    @Binding var editing: HostDraft?

    var body: some View {
        List {
            if model.isConnected {
                switch model.mode {
                case .inbox: SidebarInbox()
                case .tasks: SidebarTasks()
                case .sessions: SidebarSessions()
                }
            } else {
                NotConnected(editing: $editing)
            }
            if let error = model.hosts.storageError {
                Section { Label(error, systemImage: Icon.warning).foregroundStyle(Theme.failure) }
            }
        }
        .listStyle(.sidebar)
        .accessibilityIdentifier("sidebar-list")
        .safeAreaInset(edge: .top, spacing: 0) { header }
        .navigationTitle("Muxa")
        // Inline: a large title would sit under the mode picker.
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItemGroup(placement: .primaryAction) {
                Button("New Task", systemImage: Icon.add) { model.startNewTask() }
                    .disabled(!model.canCreateTasks)
                    .accessibilityIdentifier("sidebar-new-task")
                Button("Settings", systemImage: Icon.settings) { model.showSettings = true }
                    .accessibilityIdentifier("open-settings")
            }
        }
        .background { if model.isConnected, model.mode == .inbox { ReturnOpensItem() } }
        .onChange(of: model.session.openItems.map(\.id)) { old, new in model.itemsChanged(from: old, to: new) }
        .refreshable { await reload() }
        // tmux reports changes only for the bridge's session, so states can be stale.
        .task(id: model.isConnected ? model.mode : nil) { if model.isConnected, model.mode != .sessions { await reload() } }
    }

    /// The mode picker and the status row, above the list.
    private var header: some View {
        VStack(alignment: .leading, spacing: 6) {
            if model.isConnected {
                Picker("Mode", selection: Binding(get: { model.mode }, set: { model.setMode($0) })) {
                    ForEach(SidebarMode.allCases) { mode in
                        Text(label(mode)).tag(mode)
                    }
                }
                .pickerStyle(.segmented)
                .accessibilityIdentifier("sidebar-mode")
            }
            StatusRow()
        }
        .padding(.horizontal)
        .padding(.bottom, 6)
        .background(.bar)
    }

    /// The Inbox segment counts its items.
    private func label(_ mode: SidebarMode) -> String {
        let count = model.usesItems ? model.session.openItems.count : 0
        return mode == .inbox && count > 0 ? "Inbox \(count)" : mode.title
    }

    private func reload() async {
        try? await model.session.refresh()
        await model.session.loadTasks()
        await model.session.loadWork()
    }
}

extension View {
    /// The row that owns what the detail shows (ADR 0035 §3), also for VoiceOver.
    func sidebarHighlight(_ on: Bool) -> some View {
        listRowBackground(on ? Color.accentColor.opacity(0.15) : nil)
            .accessibilityAddTraits(on ? .isSelected : [])
    }
}

/// One line: the connection, which opens Settings. The demo offers its way out here,
/// because a demo user starts on the list, not on a pane.
struct StatusRow: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        if let host = model.currentHost ?? model.hosts.host(model.hosts.lastUsed) {
            HStack(spacing: 8) {
                Button { model.showSettings = true } label: {
                    HStack(spacing: 6) {
                        indicator
                        Text(host.title).lineLimit(1)
                    }
                    .font(.caption)
                    .foregroundStyle(.secondary)
                }
                .buttonStyle(.plain)
                .accessibilityIdentifier("status-row")
                Spacer(minLength: 0)
                if model.canLeaveDemo {
                    Button("Leave Demo") { Task { await model.leaveDemo() } }
                        .font(.caption)
                        .buttonStyle(.bordered)
                        .controlSize(.mini)
                        .accessibilityIdentifier("leave-demo")
                }
            }
        }
    }

    @ViewBuilder private var indicator: some View {
        if model.connecting || model.addingKey {
            ProgressView().controlSize(.mini)
        } else {
            Circle().fill(model.isConnected ? Theme.connected : Color.secondary.opacity(0.5))
                .frame(width: 8, height: 8)
                .accessibilityLabel(model.isConnected ? "Connected" : "Not connected")
        }
    }
}

/// The sidebar before a connection: the last host to connect to, or a first host to add.
private struct NotConnected: View {
    @Environment(AppModel.self) private var model
    @Environment(\.phoneLayout) private var phoneLayout
    @Binding var editing: HostDraft?

    var body: some View {
        if model.hosts.hosts.isEmpty {
            Section {
                Button("Add Host", systemImage: Icon.host) { editing = HostDraft() }
                    .accessibilityIdentifier("sidebar-add-host")
                // A phone starts here, so the demo is offered here too.
                Button("Try the Demo", systemImage: Icon.demo) { Task { await model.startDemo() } }
                    .accessibilityIdentifier("try-demo")
            } header: {
                Text("Add a Host")
            } footer: {
                Text("Muxa connects over SSH to the machine where your tmux sessions run. The demo shows sample sessions on this device, with no host needed.")
            }
        } else {
            // A phone has no detail column beside the list: progress and errors show here.
            if phoneLayout, model.connecting || model.addingKey || model.connectError != nil {
                Section { ConnectionState(editing: $editing).frame(maxWidth: .infinity) }
            }
            Section {
                if let host = model.hosts.host(model.hosts.lastUsed) ?? model.hosts.hosts.first {
                    Button("Connect to \(host.title)", systemImage: "bolt.horizontal") {
                        Task { await model.connect(to: host.id) }
                    }
                    .disabled(model.connecting || model.addingKey)
                    .accessibilityIdentifier("connect-last")
                }
                Button("Hosts…", systemImage: Icon.host) { model.showSettings = true }
                    .accessibilityIdentifier("open-hosts")
            }
        }
    }
}

/// Sessions mode: the manual sessions grouped by repo, with each repo's New Task and
/// Ask (ADR 0025, 0031). A row shows the session's pane in its last view.
struct SidebarSessions: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        if model.noManualSessions {
            Section("Sessions") {
                Text("No manual sessions. Open a Task's session from Inbox or Tasks.")
                    .font(.caption).foregroundStyle(.secondary)
            }
        }
        let numbers = Dictionary(uniqueKeysWithValues: model.visibleSessions.enumerated().map { ($1.id, $0 + 1) })
        ForEach(model.sessionGroups, id: \.repo) { group in
            Section {
                ForEach(group.sessions) { s in
                    Button {
                        Task { await model.open(s) }
                    } label: {
                        SessionRow(session: s, current: model.highlights(s), number: numbers[s.id])
                    }
                    .foregroundStyle(.primary)
                    .contextMenu {
                        if model.canClose(s) {
                            Button("Close Task…", systemImage: Icon.closeTask) { model.closingTask = s }
                        }
                        if model.canCloseAsk(s) {
                            Button("Close Session…", systemImage: Icon.closeTask) { model.closingAsk = s }
                        }
                    }
                }
            } header: {
                RepoHeader(repo: group.repo, newTask: newTaskAction(group), ask: askAction(group))
            }
        }
    }

    /// Only a repo the host knows the path of can get a task.
    private func newTaskAction(_ group: RepoGroup) -> (() -> Void)? {
        guard model.canCreateTasks, group.repo != nil, let dir = group.sessions.first?.repoDir else { return nil }
        return { model.startNewTask(in: dir) }
    }

    /// An ask session in a repo of a workspace (ADR 0031).
    private func askAction(_ group: RepoGroup) -> (() -> Void)? {
        guard group.repo != nil, let dir = group.sessions.first?.repoDir, model.askWorkspace(for: dir) != nil else { return nil }
        return { Task { await model.ask(about: dir) } }
    }
}

/// The detail of Sessions mode with no pane shown.
struct SessionsEmptyDetail: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        if model.paneSessions.isEmpty {
            ContentUnavailableView("No Sessions", systemImage: "rectangle.on.rectangle.slash",
                                   description: Text("No tmux session on this host has a pane to show."))
        } else {
            ContentUnavailableView("Pick a Session", systemImage: Icon.session,
                                   description: Text("Its pane shows here, as Chat, Agent or Shell."))
        }
    }
}

struct HostRow: View {
    enum State { case idle, connecting, connected }

    let host: Host
    let state: State
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            HStack {
                Image(systemName: Icon.host)
                    .foregroundStyle(.secondary)
                    .frame(width: 24)
                VStack(alignment: .leading, spacing: 2) {
                    Text(host.title).font(.body)
                    Text("\(host.target.user)@\(host.target.host)")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                }
                Spacer()
                switch state {
                case .connecting: ProgressView()
                case .connected:
                    Circle().fill(Theme.connected).frame(width: 8, height: 8)
                        .accessibilityLabel("Connected")
                case .idle: EmptyView()
                }
            }
        }
        .foregroundStyle(.primary)
        .accessibilityIdentifier("host-\(host.title)")
    }
}

/// A repo's section header; sessions without a repo tag come last.
struct RepoHeader: View {
    let repo: String?
    /// Shown as a button that starts a task in this repo.
    var newTask: (() -> Void)?
    /// Shown as a button that starts an ask session in this repo.
    var ask: (() -> Void)?

    var body: some View {
        HStack {
            if let repo {
                Label(repo, systemImage: Icon.repo)
            } else {
                Label("Other Sessions", systemImage: Icon.session)
            }
            if let repo, newTask != nil || ask != nil {
                Spacer()
                if let ask {
                    Button("Ask about \(repo)", systemImage: Icon.ask, action: ask)
                        .labelStyle(.iconOnly)
                        .accessibilityIdentifier("ask-\(repo)")
                }
                if let newTask {
                    Button("New Task in \(repo)", systemImage: Icon.add, action: newTask)
                        .labelStyle(.iconOnly)
                        .accessibilityIdentifier("new-task-\(repo)")
                }
            }
        }
    }
}
