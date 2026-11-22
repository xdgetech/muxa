import MuxaV2
import SwiftUI

/// A page of Settings below the index.
enum SettingsPlace: Hashable {
    case workspace(String)
    case workers
}

/// The list of Settings: the host's Workspaces and workers, the connection,
/// and the saved hosts. `open` shows a place.
struct SettingsIndex: View {
    @Environment(NextModel.self) private var model
    var picked: SettingsPlace?
    var open: (SettingsPlace) -> Void
    @State private var addingWorkspace = false
    @State private var editingHost: HostDraft?

    var body: some View {
        List {
            if model.isConnected { host }
            if model.link != .offline {
                Section("Connection") {
                    LabeledContent(model.connectionTitle, value: model.isConnected ? "connected" : "reconnecting")
                    Button("Disconnect", systemImage: "xmark.circle", role: .destructive) {
                        Task { await model.disconnect() }
                    }
                    .accessibilityIdentifier("settings.disconnect")
                }
                .listRowBackground(Palette.card)
            }
            HostsSection(editing: $editingHost)
        }
        .scrollContentBackground(.hidden)
        .sheet(isPresented: $addingWorkspace) { WorkspaceAddSheet() }
        .sheet(item: $editingHost) { NextHostEditor(draft: $0) }
        .refreshable { await model.settings.load() }
        .task(id: model.isConnected) { if model.isConnected { await model.settings.load() } }
    }

    @ViewBuilder
    private var host: some View {
        let s = model.settings
        Section {
            SettingsNotice()
            ForEach(s.settings?.workspaces ?? []) { w in
                row(.workspace(w.name), id: "settings.workspace.\(w.name)") {
                    VStack(alignment: .leading, spacing: 2) {
                        Text(w.name).foregroundStyle(Palette.text)
                        Text(Self.summary(w)).font(.caption).foregroundStyle(Palette.textTertiary).lineLimit(1)
                    }
                }
            }
            Button("New Workspace", systemImage: "plus") { addingWorkspace = true }
                .disabled(s.settings == nil || s.readOnly)
                .accessibilityIdentifier("settings.addworkspace")
        } header: {
            Text("Workspaces")
        } footer: {
            Text("The host's workspaces.toml and each .muxa/routines.toml. Edits by hand on the host still work.")
        }
        .listRowBackground(Palette.card)
        Section("Workers") {
            row(.workers, id: "settings.workers") {
                LabeledContent("Workers", value: "\(s.settings?.workers.count ?? 0)")
            }
        }
        .listRowBackground(Palette.card)
    }

    private func row(_ place: SettingsPlace, id: String, @ViewBuilder label: () -> some View) -> some View {
        Button { open(place) } label: {
            HStack {
                label()
                Spacer()
                Image(systemName: "chevron.right").font(.caption.weight(.bold)).foregroundStyle(Palette.textTertiary)
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .listRowBackground(picked == place ? Palette.rowSelected : Palette.card)
        .accessibilityIdentifier(id)
    }

    static func summary(_ w: SettingsWorkspace) -> String {
        var parts = [w.root]
        if !w.connections.isEmpty { parts.append(w.connections.count == 1 ? "1 connection" : "\(w.connections.count) connections") }
        if !w.mcp.isEmpty { parts.append("\(w.mcp.count) MCP") }
        if !w.routines.isEmpty { parts.append(w.routines.count == 1 ? "1 Routine" : "\(w.routines.count) Routines") }
        return parts.joined(separator: " · ")
    }
}

/// A page of Settings: a Workspace or the workers.
struct SettingsDetail: View {
    let place: SettingsPlace

    var body: some View {
        switch place {
        case .workspace(let name): WorkspaceSettings(name: name)
        case .workers: WorkersSettings()
        }
    }
}

/// Settings as one stack: the phone's tab. The Mac has its own window (`MacSettingsView`).
struct SettingsStack: View {
    @State private var path: [SettingsPlace] = []

    var body: some View {
        NavigationStack(path: $path) {
            SettingsIndex { path.append($0) }
                .background(Palette.ground)
                .navigationTitle("Settings")
                .navigationDestination(for: SettingsPlace.self) { SettingsDetail(place: $0) }
        }
        .tint(Palette.accent)
    }
}
