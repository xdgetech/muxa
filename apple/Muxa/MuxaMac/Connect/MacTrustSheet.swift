import MuxaTransport
import SwiftUI

/// A host key for the user to check before trusting it, as the Connect mock.
struct MacTrustSheet: View {
    @Environment(NextModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    let key: HostKey

    private var changed: Bool { model.hostKeyChanged }
    /// The host's name, or "the host" mid-sentence.
    private var host: String { model.currentHost?.title ?? "the host" }
    private var subject: String { model.currentHost?.title ?? "This host" }

    var body: some View {
        SheetScaffold(
            title: changed ? "Host key changed" : "New host key",
            lead: changed
                ? "\(subject) answered with a different key than the one you trusted."
                : "\(subject) is new to this Mac. Trust its key only if it matches the host.",
            primary: changed ? "Trust Anyway" : "Trust",
            primaryKind: changed ? .destructive : .primary,
            primaryID: "trust.confirm",
            cancel: {
                model.cancelTrust()
                dismiss()
            },
            confirm: {
                // The captured key: on the Mac, closing the sheet clears `untrustedKey`.
                let key = key
                dismiss()
                Task { await model.trust(key) }
            }
        ) {
            VStack(alignment: .leading, spacing: 12) {
                if changed { warning }
                VStack(alignment: .leading, spacing: 6) {
                    SectionLabel(title: "\(key.typeName) fingerprint")
                    Text(key.fingerprint)
                        .font(.system(size: 14, design: .monospaced))
                        .textSelection(.enabled)
                        .fixedSize(horizontal: false, vertical: true)
                }
                .padding(.horizontal, 14)
                .padding(.vertical, 12)
                .frame(maxWidth: .infinity, alignment: .leading)
                .raisedCard()
                check
            }
            .padding(.bottom, 4)
        }
    }

    private var warning: some View {
        HStack(alignment: .top, spacing: 10) {
            Image(systemName: Icon.warning).font(.system(size: 14, weight: .semibold)).padding(.top, 1)
            Text("If you did not reinstall or reconfigure \(host), do not trust this key. Another machine may be answering at this address.")
                .fixedSize(horizontal: false, vertical: true)
        }
        .foregroundStyle(Palette.warnText)
        .padding(.horizontal, 12)
        .padding(.vertical, 10)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Palette.warnBack, in: RoundedRectangle(cornerRadius: 10))
        .overlay(RoundedRectangle(cornerRadius: 10).strokeBorder(Palette.warnBorder))
        .accessibilityElement(children: .combine)
    }

    private var check: some View {
        let command = Text("ssh-keygen -lf /etc/ssh/\(key.hostKeyFile)")
            .font(.system(size: 12, design: .monospaced))
            .foregroundStyle(Palette.textMid)
        return Text("On \(host), run \(command) and check that it prints the same.")
            .font(.system(size: 12))
            .foregroundStyle(Palette.textTertiary)
            .textSelection(.enabled)
            .fixedSize(horizontal: false, vertical: true)
    }
}

private extension HostKey {
    /// The algorithm word of the OpenSSH key type: `ssh-ed25519` is ED25519.
    var typeName: String {
        let type = openSSH.split(separator: " ").first.map(String.init) ?? ""
        if type.contains("ed25519") { return "ED25519" }
        if type.contains("ecdsa") { return "ECDSA" }
        if type.contains("rsa") { return "RSA" }
        return "SHA256"
    }

    /// The server's public key file of that type, for `ssh-keygen -lf`.
    var hostKeyFile: String {
        switch typeName {
        case "ECDSA": "ssh_host_ecdsa_key.pub"
        case "RSA": "ssh_host_rsa_key.pub"
        default: "ssh_host_ed25519_key.pub"
        }
    }
}
