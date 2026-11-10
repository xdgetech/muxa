import MuxaSession
import SwiftUI

/// Settings (ADR 0035 §4): the hosts, the demo and the version. Hosts change rarely,
/// so they live here and not in the sidebar.
struct SettingsScreen: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var editing: HostDraft?

    var body: some View {
        NavigationStack {
            List {
                hosts
                if model.usesPolicies {
                    Section {
                        NavigationLink {
                            PoliciesList()
                        } label: {
                            Label("Policies", systemImage: Icon.policy)
                        }
                        .accessibilityIdentifier("settings-policies")
                    } footer: {
                        Text("What you told the overall assistant on this host.")
                    }
                }
                demo
                Section {
                    LabeledContent("Version", value: Self.version)
                }
                if let error = model.hosts.storageError {
                    Section { Label(error, systemImage: Icon.warning).foregroundStyle(Theme.failure) }
                }
            }
            .navigationTitle("Settings")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done") { dismiss() }.accessibilityIdentifier("settings-done")
                }
                if !model.hosts.hosts.isEmpty, !model.demo {
                    ToolbarItem(placement: .topBarLeading) { EditButton() }
                }
            }
            .sheet(item: $editing) { draft in HostEditor(draft: draft) }
        }
    }

    private var hosts: some View {
        Section {
            ForEach(model.hosts.hosts) { host in
                HostRow(host: host, state: state(of: host)) { Task { await model.connect(to: host.id) } }
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
            Button("Add Host", systemImage: Icon.add) { editing = HostDraft() }
                .accessibilityIdentifier("settings-add-host")
            if model.isConnected {
                Button("Disconnect", systemImage: Icon.disconnect, role: .destructive) {
                    Task { await model.disconnect() }
                }
                .accessibilityIdentifier("settings-disconnect")
            }
        } header: {
            Text("Hosts")
        } footer: {
            Text("Tap a host to connect. Muxa connects over SSH to the machine where your tmux sessions run.")
        }
    }

    @ViewBuilder private var demo: some View {
        if model.canLeaveDemo {
            Section {
                Button("Leave Demo", systemImage: Icon.leaveDemo) {
                    dismiss()
                    Task { await model.leaveDemo() }
                }
                .accessibilityIdentifier("settings-leave-demo")
            } footer: {
                Text("Hosts added during the demo are not saved.")
            }
        } else if !model.demo {
            Section {
                Button("Try the Demo", systemImage: Icon.demo) {
                    dismiss()
                    Task { await model.startDemo() }
                }
                .accessibilityIdentifier("settings-try-demo")
            } footer: {
                Text("Sample sessions on this device, with no host needed.")
            }
        }
    }

    private func state(of host: Host) -> HostRow.State {
        guard host.id == model.currentHostID else { return .idle }
        if model.connecting || model.addingKey { return .connecting }
        return model.isConnected ? .connected : .idle
    }

    private static var version: String {
        let info = Bundle.main.infoDictionary
        let short = info?["CFBundleShortVersionString"] as? String ?? "?"
        let build = info?["CFBundleVersion"] as? String ?? "?"
        return "\(short) (\(build))"
    }
}
