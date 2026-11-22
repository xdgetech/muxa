import MuxaV2
import SwiftUI

/// The connection to the host and the saved hosts, on the right of Settings.
struct MacConnectionPane: View {
    @Environment(NextModel.self) private var model
    @State private var editingHost: HostDraft?

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            MacPaneTitle(title: model.connectionTitle, subtitle: status)
            PaletteForm {
                if model.link != .offline {
                    Section {
                        Button("Disconnect", systemImage: "xmark.circle", role: .destructive) {
                            Task { await model.disconnect() }
                        }
                        .accessibilityIdentifier("settings.disconnect")
                    }
                }
                HostsSection(editing: $editingHost)
            }
            .frame(maxWidth: MacSettingsLayout.formWidth)
            .padding(.horizontal, 8)
        }
        .sheet(item: $editingHost) { MacHostEditor(draft: $0) }
    }

    private var status: String {
        switch model.link {
        case .connected: "Connected"
        case .connecting: "Connecting…"
        case .reconnecting: "Reconnecting…"
        case .failed(let why): "Not connected: \(why)"
        case .offline: "Not connected. Pick a host below."
        }
    }
}
