import SwiftUI

/// New Host and Edit Host on the Mac, as the Connect mock: the address in one
/// card, the password of the first connection, and the rest under Advanced.
struct MacHostEditor: View {
    @Environment(NextModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State var draft: HostDraft
    @State private var password = ""
    @State private var advanced = false

    var body: some View {
        SheetScaffold(
            title: draft.isNew ? "New Host" : "Edit Host",
            lead: "A Mac or Linux machine where the muxa server runs.",
            primary: connects ? "Connect" : "Save",
            primaryDisabled: !draft.isValid,
            primaryID: "host.save",
            cancel: { dismiss() },
            confirm: confirm
        ) {
            VStack(alignment: .leading, spacing: 14) {
                hostCard
                firstConnection
                DisclosureGroup(isExpanded: $advanced) {
                    MacHostAdvanced(draft: $draft, dismiss: { dismiss() })
                        .padding(.top, 10)
                } label: {
                    Text("Advanced").foregroundStyle(Palette.textSecondary)
                }
                .accessibilityIdentifier("host.advanced")
            }
            .padding(.bottom, 4)
        }
    }

    private var hostCard: some View {
        VStack(spacing: 0) {
            FieldRow(label: "Name") {
                TextField("Name", text: $draft.name, prompt: Text("Mac mini").foregroundStyle(Palette.textDim))
                    .font(.system(size: 14))
                    .accessibilityIdentifier("host-name")
            }
            FieldRow(label: "Address") {
                TextField("Address", text: $draft.address, prompt: Text("Hostname or IP address").foregroundStyle(Palette.textDim))
                    .font(.system(size: 14, design: .monospaced))
                    .autocorrectionDisabled()
                    .accessibilityIdentifier("host-address")
            }
            FieldRow(label: "User", divider: false) {
                HStack(spacing: 12) {
                    TextField("User", text: $draft.user, prompt: Text("Your login on the host").foregroundStyle(Palette.textDim))
                        .font(.system(size: 14, design: .monospaced))
                        .autocorrectionDisabled()
                        .textContentType(.username)
                        .accessibilityIdentifier("host-user")
                    TextField("Port", text: $draft.port)
                        .font(.system(size: 13, design: .monospaced))
                        .multilineTextAlignment(.trailing)
                        .padding(.horizontal, 8)
                        .frame(width: 72, height: 28)
                        .background(Palette.ground, in: RoundedRectangle(cornerRadius: 6))
                        .overlay(RoundedRectangle(cornerRadius: 6).strokeBorder(Palette.borderStrong))
                        .help("Port")
                        .accessibilityLabel("Port")
                        .accessibilityIdentifier("host-port")
                }
            }
        }
        .raisedCard()
    }

    private var firstConnection: some View {
        VStack(alignment: .leading, spacing: 6) {
            SectionLabel(title: "First connection").padding(.horizontal, 4)
            FieldRow(label: "Password", divider: false) {
                SecureField("Password", text: $password, prompt: Text("Only for the first connection").foregroundStyle(Palette.textDim))
                    .font(.system(size: 14))
                    .textContentType(.password)
                    .accessibilityIdentifier("host-password")
            }
            .raisedCard()
            Text("Used once to add this Mac's key, which can only run muxa-bridge. The password is not kept.")
                .font(.system(size: 12))
                .foregroundStyle(Palette.textTertiary)
                .fixedSize(horizontal: false, vertical: true)
                .padding(.horizontal, 4)
        }
    }

    /// A new host connects at once; an edited one connects only to add the key.
    private var connects: Bool { draft.isNew || !password.isEmpty }

    private func confirm() {
        guard draft.isValid else { return }
        if connects {
            let entered = password
            password = ""
            dismiss()
            model.connect(draft, password: entered)
        } else {
            model.save(draft)
            dismiss()
        }
    }
}

/// The host editor's Advanced part: what most hosts never need.
private struct MacHostAdvanced: View {
    @Environment(NextModel.self) private var model
    @Binding var draft: HostDraft
    let dismiss: () -> Void
    @State private var copied = false

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            VStack(alignment: .leading, spacing: 6) {
                FieldRow(label: "Bridge", divider: false) {
                    TextField("Bridge command", text: $draft.command)
                        .font(.system(size: 13, design: .monospaced))
                        .autocorrectionDisabled()
                        .accessibilityIdentifier("host-command")
                }
                .raisedCard()
                caption("Where muxa-bridge is installed on the host. Change it only if you installed it elsewhere.")
            }
            if let line = model.authorizedKeyLine(command: draft.command) {
                keyLine(line)
            }
            if let key = draft.hostKey, model.keepsHostKey(draft) {
                trustedKey(key.fingerprint)
            }
            if !draft.isNew {
                Button("Delete Host", role: .destructive) {
                    model.hosts.delete(draft.id)
                    dismiss()
                }
                .buttonStyle(ActionButtonStyle(kind: .destructive, compact: true))
                .accessibilityIdentifier("host.delete")
            }
        }
    }

    private func keyLine(_ line: String) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            SectionLabel(title: "This Mac's key").padding(.horizontal, 4)
            VStack(alignment: .leading, spacing: 10) {
                Text(line)
                    .font(.system(size: 11, design: .monospaced))
                    .foregroundStyle(Palette.textMid)
                    .textSelection(.enabled)
                    .fixedSize(horizontal: false, vertical: true)
                Button { Clipboard.copy(line); copied = true } label: {
                    Label(copied ? "Copied" : "Copy Key Line", systemImage: copied ? "checkmark" : "doc.on.doc")
                }
                .buttonStyle(ActionButtonStyle(kind: .secondary, compact: true))
            }
            .padding(.horizontal, 14)
            .padding(.vertical, 12)
            .frame(maxWidth: .infinity, alignment: .leading)
            .raisedCard()
            caption("Or add this line to ~/.ssh/authorized_keys on the host yourself.")
        }
    }

    private func trustedKey(_ fingerprint: String) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            SectionLabel(title: "Trusted host key").padding(.horizontal, 4)
            HStack(spacing: 12) {
                Text(fingerprint)
                    .font(.system(size: 12, design: .monospaced))
                    .textSelection(.enabled)
                    .fixedSize(horizontal: false, vertical: true)
                    .frame(maxWidth: .infinity, alignment: .leading)
                Button("Forget") { draft.hostKey = nil }
                    .buttonStyle(ActionButtonStyle(kind: .secondary, compact: true))
                    .accessibilityIdentifier("host.forgetkey")
            }
            .padding(.horizontal, 14)
            .padding(.vertical, 10)
            .raisedCard()
        }
    }

    private func caption(_ text: String) -> some View {
        Text(text)
            .font(.system(size: 12))
            .foregroundStyle(Palette.textTertiary)
            .fixedSize(horizontal: false, vertical: true)
            .padding(.horizontal, 4)
    }
}
