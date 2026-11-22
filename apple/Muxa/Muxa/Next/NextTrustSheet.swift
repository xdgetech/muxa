import MuxaTransport
import SwiftUI

/// Shows a host key for the user to check before trusting it.
struct NextTrustSheet: View {
    @Environment(NextModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    let key: HostKey

    private var changed: Bool { model.hostKeyChanged }

    var body: some View {
        NavigationStack {
            PaletteForm {
                if changed {
                    Section {
                        Label("If you did not reinstall or reconfigure the host, do not trust this key. Another machine may be answering at this address.",
                              systemImage: Icon.warning)
                            .foregroundStyle(Theme.failure)
                            .fixedSize(horizontal: false, vertical: true)
                    }
                }
                Section {
                    Text(key.fingerprint).font(.body.monospaced()).textSelection(.enabled)
                } header: {
                    Text(changed ? "Host Key Changed" : "New Host Key")
                } footer: {
                    Text("Check that it matches the output of `ssh-keygen -lf /etc/ssh/ssh_host_ed25519_key.pub` on the host.")
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
            .navigationTitle(model.currentHost?.title ?? "Host Key")
            #if os(macOS)
            .frame(minWidth: 460, minHeight: 260)
            #endif
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") {
                        model.cancelTrust()
                        dismiss()
                    }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button(changed ? "Trust Anyway" : "Trust", role: changed ? .destructive : nil) {
                        dismiss()
                        Task { await model.trust(key) }
                    }
                }
            }
        }
    }
}

extension NextModel {
    /// The host had a trusted key, and now presents another one.
    var hostKeyChanged: Bool { currentHost?.target.hostKey != nil }
}
