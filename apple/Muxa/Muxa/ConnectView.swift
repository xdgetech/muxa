import MuxaSession
import MuxaTransport
import SwiftUI

struct ConnectView: View {
    @Environment(AppModel.self) private var model
    @State private var host = ""
    @State private var port = "22"
    @State private var user = ""
    @State private var command = "~/.local/bin/muxa-bridge"
    @State private var connecting = false

    var body: some View {
        NavigationStack {
            Form {
                Section("Host") {
                    TextField("Host or Tailscale name", text: $host)
                        .textContentType(.URL)
                        .textInputAutocapitalization(.never)
                        .autocorrectionDisabled()
                    TextField("Port", text: $port)
                        .keyboardType(.numberPad)
                    TextField("User", text: $user)
                        .textInputAutocapitalization(.never)
                        .autocorrectionDisabled()
                    TextField("Bridge command", text: $command)
                        .textInputAutocapitalization(.never)
                        .autocorrectionDisabled()
                        .font(.body.monospaced())
                }
                Section {
                    Button(connecting ? "Connecting…" : "Connect") { Task { await connect() } }
                        .disabled(connecting || host.isEmpty || user.isEmpty || Int(port) == nil)
                        .keyboardShortcut(.defaultAction)
                    if let error = model.connectError ?? reason {
                        Text(error).foregroundStyle(.red)
                    }
                }
                Section {
                    if let line = model.authorizedKeyLine() {
                        Text(line)
                            .font(.caption.monospaced())
                            .textSelection(.enabled)
                        Button("Copy") { UIPasteboard.general.string = line }
                    }
                } header: {
                    Text("This iPad's key")
                } footer: {
                    Text("Add this line to ~/.ssh/authorized_keys on the host. It lets the key run only the bridge.")
                }
                if let hostKey = model.target?.hostKey {
                    Section("Trusted host key") {
                        Text(hostKey.fingerprint).font(.caption.monospaced())
                    }
                }
            }
            .navigationTitle("muxa")
            .onAppear(perform: loadTarget)
        }
    }

    /// Why the last connection ended, unless the user left on purpose.
    private var reason: String? {
        if case .disconnected(let reason) = model.session.phase, reason != "disconnected" { return reason }
        return nil
    }

    private func loadTarget() {
        guard let t = model.target else { return }
        host = t.host
        port = String(t.port)
        user = t.user
        command = t.command
    }

    private func connect() async {
        connecting = true
        defer { connecting = false }
        let trimmed = host.trimmingCharacters(in: .whitespaces)
        // A new host or user starts without a trusted key.
        let sameHost = model.target?.host == trimmed && model.target?.port == Int(port)
        model.target = SSHTarget(
            host: trimmed, port: Int(port) ?? 22, user: user, command: command,
            hostKey: sameHost ? model.target?.hostKey : nil)
        await model.connect()
    }
}

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
                    Text(model.target?.hostKey == nil ? "New host" : "The host key changed")
                } footer: {
                    Text("Compare with `ssh-keygen -lf /etc/ssh/ssh_host_ed25519_key.pub` on the host.")
                }
            }
            .navigationTitle(model.target?.host ?? "Host key")
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") {
                        model.untrustedKey = nil
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
