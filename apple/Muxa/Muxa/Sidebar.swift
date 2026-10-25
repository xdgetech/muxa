import MuxaProtocol
import MuxaSession
import SwiftUI

/// Saved hosts, and the connected host's sessions below them.
struct Sidebar: View {
    @Environment(AppModel.self) private var model
    @Binding var editing: HostDraft?

    var body: some View {
        List {
            Section("Hosts") {
                ForEach(model.hosts.hosts) { host in
                    HostRow(host: host, state: state(of: host)) { connect(host) }
                        .contextMenu {
                            Button("Edit", systemImage: "pencil") { editing = HostDraft(host) }
                            Button("Delete", systemImage: "trash", role: .destructive) { model.hosts.delete(host.id) }
                        }
                        .swipeActions {
                            Button("Delete", systemImage: "trash", role: .destructive) { model.hosts.delete(host.id) }
                            Button("Edit", systemImage: "pencil") { editing = HostDraft(host) }
                        }
                }
                .onMove { model.hosts.move(fromOffsets: $0, toOffset: $1) }
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
                        }
                    } header: {
                        RepoHeader(repo: group.repo)
                    }
                }
            }
            if let error = model.hosts.storageError {
                Section { Label(error, systemImage: "exclamationmark.triangle").foregroundStyle(.red) }
            }
        }
        .listStyle(.sidebar)
        .navigationTitle("muxa")
        .toolbar {
            ToolbarItem(placement: .primaryAction) {
                Button("Add Host", systemImage: "plus") { editing = HostDraft() }
            }
            ToolbarItem(placement: .secondaryAction) { EditButton() }
        }
        .refreshable { try? await model.session.refresh() }
    }

    private func state(of host: Host) -> HostRow.State {
        guard host.id == model.currentHostID else { return .idle }
        if model.connecting { return .connecting }
        return model.isConnected ? .connected : .idle
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
                Image(systemName: "desktopcomputer")
                    .foregroundStyle(state == .connected ? Color.green : Color.secondary)
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
                case .connected: Image(systemName: "checkmark.circle.fill").foregroundStyle(.green)
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

    var body: some View {
        if let repo {
            Label(repo, systemImage: "folder")
        } else {
            Label("Other Sessions", systemImage: "rectangle.stack")
        }
    }
}
