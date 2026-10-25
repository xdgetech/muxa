import MuxaTransport
import SwiftUI

struct TrustHostSheet: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    let key: HostKey

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    Text(key.fingerprint).font(.body.monospaced()).textSelection(.enabled)
                } header: {
                    Text(model.currentHost?.target.hostKey == nil ? "New host" : "The host key changed")
                } footer: {
                    Text("Compare with `ssh-keygen -lf /etc/ssh/ssh_host_ed25519_key.pub` on the host.")
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
                    Button("Trust") {
                        dismiss()
                        Task { await model.trust(key) }
                    }
                }
            }
        }
    }
}
