import MuxaSession
import MuxaTransport
import SwiftUI
import UIKit

/// The editable fields of a host, as text. A new host has no saved id yet.
struct HostDraft: Identifiable {
    let id: UUID
    let isNew: Bool
    var name = ""
    var address = ""
    var port = "22"
    var user = ""
    var command = SSHTarget.defaultCommand
    var hostKey: HostKey?

    init() {
        id = UUID()
        isNew = true
    }

    init(_ host: Host) {
        id = host.id
        isNew = false
        name = host.name
        address = host.target.host
        port = String(host.target.port)
        user = host.target.user
        command = host.target.command
        hostKey = host.target.hostKey
    }

    var trimmedAddress: String { address.trimmingCharacters(in: .whitespaces) }
    var trimmedUser: String { user.trimmingCharacters(in: .whitespaces) }

    var isValid: Bool {
        guard let p = Int(port), (1...65535).contains(p) else { return false }
        return !trimmedAddress.isEmpty && !trimmedUser.isEmpty && !command.isEmpty
            && !command.contains(where: { $0 == "\"" || $0.isNewline })
    }

    /// The host to save. A changed address, port or user drops the trusted host key:
    /// it belonged to the old server.
    func host(replacing old: Host?) -> Host {
        let target = SSHTarget(host: trimmedAddress, port: Int(port) ?? 22, user: trimmedUser, command: command)
        var host = Host(id: id, name: name.trimmingCharacters(in: .whitespaces), target: target)
        if let old, old.target.host == target.host, old.target.port == target.port, old.target.user == target.user {
            host.target.hostKey = hostKey
        }
        return host
    }
}

struct HostEditor: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State var draft: HostDraft
    @State private var password = ""
    @State private var copied = false

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    TextField("Name", text: $draft.name, prompt: Text("Mac mini"))
                        .accessibilityIdentifier("host-name")
                    TextField("Address", text: $draft.address, prompt: Text("Host or Tailscale name"))
                        .textContentType(.URL)
                        .keyboardType(.URL)
                        .accessibilityIdentifier("host-address")
                        .textInputAutocapitalization(.never)
                        .autocorrectionDisabled()
                    TextField("User", text: $draft.user)
                        .textContentType(.username)
                        .accessibilityIdentifier("host-user")
                        .textInputAutocapitalization(.never)
                        .autocorrectionDisabled()
                    TextField("Port", text: $draft.port)
                        .keyboardType(.numberPad)
                } header: {
                    Text("Host")
                }
                Section {
                    TextField("Bridge command", text: $draft.command)
                        .textInputAutocapitalization(.never)
                        .autocorrectionDisabled()
                        .font(.body.monospaced())
                } header: {
                    Text("Bridge")
                } footer: {
                    Text("Where muxa-bridge is installed on the host.")
                }
                Section {
                    SecureField("Password for \(draft.trimmedUser.isEmpty ? "the user" : draft.trimmedUser)", text: $password)
                        .textContentType(.password)
                    Button("Add This iPad's Key and Connect") { setUp() }
                        .disabled(!draft.isValid || password.isEmpty)
                } header: {
                    Text("First Connection")
                } footer: {
                    Text("Logs in once with your password and adds this iPad's key to ~/.ssh/authorized_keys, restricted to the bridge. The password is not stored.")
                }
                if let line = model.authorizedKeyLine(command: draft.command) {
                    Section {
                        Text(line)
                            .font(.caption.monospaced())
                            .textSelection(.enabled)
                        Button(copied ? "Copied" : "Copy Key Line", systemImage: copied ? "checkmark" : "doc.on.doc") {
                            UIPasteboard.general.string = line
                            copied = true
                        }
                    } header: {
                        Text("This iPad's Key")
                    } footer: {
                        Text("Or add this line to ~/.ssh/authorized_keys on the host yourself.")
                    }
                }
                if let key = draft.hostKey {
                    Section("Trusted Host Key") {
                        LabeledContent("Fingerprint") {
                            Text(key.fingerprint).font(.caption.monospaced()).textSelection(.enabled)
                        }
                        Button("Forget Host Key", role: .destructive) { draft.hostKey = nil }
                    }
                }
                if !draft.isNew {
                    Section {
                        Button("Delete Host", role: .destructive) {
                            model.hosts.delete(draft.id)
                            dismiss()
                        }
                    }
                }
            }
            .navigationTitle(draft.isNew ? "New Host" : "Edit Host")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Save") {
                        save()
                        dismiss()
                    }
                    .disabled(!draft.isValid)
                }
            }
        }
    }

    private func save() {
        model.hosts.upsert(draft.host(replacing: model.hosts.host(draft.id)))
    }

    /// Saves, closes the editor, then runs the one-time key setup. The trust sheet
    /// and any error appear on the main screen.
    private func setUp() {
        save()
        let entered = password
        password = ""
        let id = draft.id
        dismiss()
        Task { await model.setUpKey(on: id, password: entered) }
    }
}
