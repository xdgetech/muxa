import SwiftUI

/// Hosts and sessions in the sidebar, the pane in the detail. The sidebar hides
/// once a pane is open, so the pane gets the full width.
struct RootView: View {
    @Environment(AppModel.self) private var model
    @State private var columns: NavigationSplitViewVisibility = .all
    @State private var editing: HostDraft?

    var body: some View {
        @Bindable var model = model
        NavigationSplitView(columnVisibility: $columns) {
            Sidebar(editing: $editing)
        } detail: {
            DetailView(editing: $editing)
        }
        .navigationSplitViewStyle(.balanced)
        .onChange(of: model.session.watchedPane) { _, pane in
            if pane != nil, model.isConnected { columns = .detailOnly }
        }
        .onChange(of: model.isConnected) { _, connected in
            if !connected { columns = .all }
        }
        .sheet(item: $editing, onDismiss: model.refocusPane) { draft in
            HostEditor(draft: draft)
        }
        .sheet(item: $model.untrustedKey, onDismiss: model.refocusPane) { key in
            // Only Trust or Cancel closes it, so a held setup password is always cleared.
            TrustHostSheet(key: key).interactiveDismissDisabled()
        }
        .sheet(isPresented: $model.showSwitcher, onDismiss: model.refocusPane) {
            SessionSwitcher()
        }
    }
}

/// The pane when one is open; otherwise what to do next.
struct DetailView: View {
    @Environment(AppModel.self) private var model
    @Binding var editing: HostDraft?

    var body: some View {
        if model.isConnected, model.session.watchedPane != nil {
            PaneScreen()
        } else if model.connecting, let host = model.currentHost {
            ProgressView("Connecting to \(host.title)…")
        } else if let error = model.connectError ?? reason, let host = model.currentHost {
            ContentUnavailableView {
                Label("Can't Connect to \(host.title)", systemImage: "exclamationmark.triangle")
            } description: {
                Text(error)
            } actions: {
                Button("Try Again") { Task { await model.connect(to: host.id) } }
                    .buttonStyle(.borderedProminent)
                Button("Edit Host") { editing = HostDraft(host) }
            }
        } else if model.isConnected {
            ContentUnavailableView("No Sessions", systemImage: "rectangle.on.rectangle.slash",
                                   description: Text("No tmux session on this host has a pane to show."))
        } else if model.hosts.hosts.isEmpty {
            ContentUnavailableView {
                Label("Add a Host", systemImage: "desktopcomputer")
            } description: {
                Text("muxa connects over SSH to the Mac or Linux machine where your tmux sessions run.")
            } actions: {
                Button("Add Host") { editing = HostDraft() }.buttonStyle(.borderedProminent)
            }
        } else {
            ContentUnavailableView("Not Connected", systemImage: "bolt.horizontal",
                                   description: Text("Pick a host in the sidebar."))
        }
    }

    /// Why the last connection ended, unless the user left on purpose.
    private var reason: String? {
        if case .disconnected(let reason) = model.session.phase, reason != "disconnected" { return reason }
        return nil
    }
}
