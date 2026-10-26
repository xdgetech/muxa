import MuxaTransport
import SwiftUI

struct TrustHostSheet: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    let key: HostKey

    /// The host had a trusted key, and now presents another one.
    private var changed: Bool { model.currentHost?.target.hostKey != nil }

    var body: some View {
        NavigationStack {
            Form {
                if changed {
                    Section {
                        Label("If you did not reinstall or reconfigure the host, do not trust this key. Another machine may be answering at this address.",
                              systemImage: Icon.warning)
                            .foregroundStyle(Theme.failure)
                    }
                }
                Section {
                    Text(key.fingerprint).font(.body.monospaced()).textSelection(.enabled)
                } header: {
                    Text(changed ? "Host Key Changed" : "New Host Key")
                } footer: {
                    Text("Check that it matches the output of `ssh-keygen -lf /etc/ssh/ssh_host_ed25519_key.pub` on the host.")
                }
            }
            .navigationTitle(model.currentHost?.title ?? "Host Key")
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
