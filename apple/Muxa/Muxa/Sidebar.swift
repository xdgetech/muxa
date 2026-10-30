import MuxaProtocol
import MuxaSession
import SwiftUI

/// Saved hosts, and the connected host's sessions below them.
struct Sidebar: View {
    @Environment(AppModel.self) private var model
    @Binding var editing: HostDraft?

    var body: some View {
        List {
            // No empty header on first launch. A phone starts here, so the demo is offered here too.
            if model.hosts.hosts.isEmpty {
                Section {
                    Button("Try the Demo", systemImage: Icon.demo) { Task { await model.startDemo() } }
                        .accessibilityIdentifier("try-demo")
                } footer: {
                    Text("Sample sessions on this device, with no host needed.")
                }
            }
            if model.canLeaveDemo {
                Section {
                    Button("Leave Demo", systemImage: Icon.leaveDemo) { Task { await model.leaveDemo() } }
                        .accessibilityIdentifier("leave-demo")
                } footer: {
                    Text("Hosts added during the demo are not saved.")
                }
            }
            if !model.hosts.hosts.isEmpty {
                Section("Hosts") {
                    ForEach(model.hosts.hosts) { host in
                        HostRow(host: host, state: state(of: host)) { connect(host) }
                            .contextMenu {
                                Button("Edit", systemImage: Icon.edit) { editing = HostDraft(host) }
                                Button("Delete", systemImage: Icon.delete, role: .destructive) { model.hosts.delete(host.id) }
                            }
                            .swipeActions {
                                Button("Delete", systemImage: Icon.delete, role: .destructive) { model.hosts.delete(host.id) }
                                Button("Edit", systemImage: Icon.edit) { editing = HostDraft(host) }
                            }
                    }
                    .onMove { model.hosts.move(fromOffsets: $0, toOffset: $1) }
                }
            }
            if model.isConnected {
                let numbers = Dictionary(
                    uniqueKeysWithValues: model.visibleSessions.enumerated().map { ($1.id, $0 + 1) })
                ForEach(model.sessionGroups, id: \.repo) { group in
                    Section {
                        ForEach(group.sessions) { s in
                            Button {
                                Task { await model.open(s) }
                            } label: {
                                SessionRow(session: s, current: s.id == model.currentSession?.id, number: numbers[s.id])
                            }
                            .foregroundStyle(.primary)
                            .contextMenu {
                                if model.canClose(s) {
                                    Button("Close Task…", systemImage: Icon.closeTask) { model.closingTask = s }
                                }
                            }
                        }
                    } header: {
                        RepoHeader(repo: group.repo, newTask: newTaskAction(group))
                    }
                }
            }
            if let error = model.hosts.storageError {
                Section { Label(error, systemImage: Icon.warning).foregroundStyle(Theme.failure) }
            }
        }
        .listStyle(.sidebar)
        .navigationTitle("Muxa")
        .toolbar {
            ToolbarItem(placement: .primaryAction) {
                Button("Add Host", systemImage: Icon.add) { editing = HostDraft() }
            }
            if !model.hosts.hosts.isEmpty, !model.demo {
                ToolbarItem(placement: .secondaryAction) { EditButton() }
            }
        }
        .refreshable { try? await model.session.refresh() }
    }

    private func state(of host: Host) -> HostRow.State {
        guard host.id == model.currentHostID else { return .idle }
        if model.connecting || model.addingKey { return .connecting }
        return model.isConnected ? .connected : .idle
    }

    /// Only a repo the host knows the path of can get a task.
    private func newTaskAction(_ group: RepoGroup) -> (() -> Void)? {
        guard model.canStartTasks, group.repo != nil, let dir = group.sessions.first?.repoDir else { return nil }
        return { model.startNewTask(in: dir) }
    }

    private func connect(_ host: Host) {
        Task { await model.connect(to: host.id) }
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

    var body: some View {
        HStack {
            if let repo {
                Label(repo, systemImage: Icon.repo)
            } else {
                Label("Other Sessions", systemImage: Icon.session)
            }
            if let newTask, let repo {
                Spacer()
                Button("New Task in \(repo)", systemImage: Icon.add, action: newTask)
                    .labelStyle(.iconOnly)
                    .accessibilityIdentifier("new-task-\(repo)")
            }
        }
    }
}
