import MuxaSession
import SwiftUI

/// The Mac before it is connected, as the Connect mock: This Mac, the hosts
/// and Try the Demo in one centered column, the link's state in the header.
struct MacConnectView: View {
    @Environment(NextModel.self) private var model
    @State private var editing: HostDraft?

    var body: some View {
        VStack(spacing: 0) {
            MacHeaderBar {
                HStack(spacing: 10) {
                    LogoTile()
                    Text("Connect").font(.system(size: 15, weight: .semibold))
                }
            } trailing: {
                status
            }
            ScrollView {
                VStack(alignment: .leading, spacing: 18) {
                    if model.serverIsLocal { thisMac }
                    hosts
                    demo
                }
                .frame(width: 560)
                .padding(.vertical, 28)
                .frame(maxWidth: .infinity)
            }
        }
        .font(.system(size: 13))
        .foregroundStyle(Palette.text)
        .background(Palette.ground)
        .sheet(item: $editing) { MacHostEditor(draft: $0) }
        .sheet(item: Bindable(model).untrustedKey) { MacTrustSheet(key: $0) }
    }

    /// Connection errors show here, so a long reason wraps instead of vanishing.
    private var status: some View {
        Text(model.linkStatus)
            .foregroundStyle(statusColor)
            .lineLimit(2)
            .multilineTextAlignment(.trailing)
            .frame(maxWidth: 560, alignment: .trailing)
            .textSelection(.enabled)
            .help(model.linkStatus)
            .accessibilityIdentifier("next.link")
    }

    private var statusColor: Color {
        switch model.link {
        case .failed: Palette.warnText
        case .connected: Palette.okText
        default: Palette.textTertiary
        }
    }

    private var thisMac: some View {
        VStack(alignment: .leading, spacing: 6) {
            SectionLabel(title: "This Mac").padding(.horizontal, 4)
            Button { Task { await model.connectLocal() } } label: {
                HStack(spacing: 12) {
                    Image(systemName: "display").font(.system(size: 16))
                    VStack(alignment: .leading, spacing: 2) {
                        Text("This Mac").font(.system(size: 14, weight: .semibold))
                        Text("The muxa server runs here. No SSH.")
                            .font(.system(size: 12))
                            .foregroundStyle(Palette.textTertiary)
                    }
                    Spacer(minLength: 0)
                }
                .padding(.horizontal, 14)
                .frame(minHeight: 52)
                .contentShape(Rectangle())
                .raisedCard()
            }
            .buttonStyle(.plain)
            .accessibilityElement(children: .combine)
            .accessibilityIdentifier("next.local")
        }
    }

    private var hosts: some View {
        VStack(alignment: .leading, spacing: 6) {
            SectionLabel(title: "Hosts").padding(.horizontal, 4)
            VStack(spacing: 0) {
                ForEach(model.hosts.hosts) { host in
                    MacHostRow(host: host, connecting: connecting(host)) { editing = HostDraft(host) }
                }
                Button { editing = HostDraft() } label: {
                    HStack(spacing: 10) {
                        Image(systemName: Icon.add).font(.system(size: 13, weight: .semibold))
                        Text("Add Host")
                        Spacer(minLength: 0)
                    }
                    .font(.system(size: 14))
                    .foregroundStyle(Palette.accent)
                    .padding(.horizontal, 14)
                    .frame(minHeight: 48)
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .accessibilityIdentifier("next.addhost")
            }
            .raisedCard()
            .clipShape(RoundedRectangle(cornerRadius: 10))
        }
    }

    private var demo: some View {
        Button("Try the Demo") { Task { await model.startDemo() } }
            .buttonStyle(.plain)
            .foregroundStyle(Palette.textMid)
            .padding(.horizontal, 12)
            .frame(height: 34)
            .overlay(RoundedRectangle(cornerRadius: 8).strokeBorder(Palette.borderStrong))
            .contentShape(Rectangle())
            .help("The demo runs in the app. Nothing reaches a host.")
            .accessibilityIdentifier("next.demo")
    }

    private func connecting(_ host: MuxaSession.Host) -> Bool {
        model.link == .connecting && model.currentHost?.id == host.id
    }
}

/// One saved host: tap to connect, the gear to edit.
private struct MacHostRow: View {
    @Environment(NextModel.self) private var model
    let host: MuxaSession.Host
    let connecting: Bool
    let edit: () -> Void

    var body: some View {
        HStack(spacing: 12) {
            Circle()
                .fill(connecting ? Theme.attention : Palette.textDim)
                .frame(width: 8, height: 8)
                .accessibilityHidden(true)
            Button { Task { await model.connect(to: host.id) } } label: {
                VStack(alignment: .leading, spacing: 2) {
                    Text(host.title).font(.system(size: 14, weight: .semibold))
                    Text(address)
                        .font(.system(size: 12, design: .monospaced))
                        .foregroundStyle(Palette.textTertiary)
                        .lineLimit(1)
                        .truncationMode(.middle)
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.vertical, 8)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityElement(children: .ignore)
            .accessibilityLabel(host.title)
            .accessibilityValue(address)
            .accessibilityAddTraits(.isButton)
            Button(action: edit) {
                Image(systemName: Icon.settings)
                    .font(.system(size: 14))
                    .foregroundStyle(Palette.textSecondary)
                    .frame(width: 36, height: 36)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .help("Edit \(host.title)")
            .accessibilityLabel("Edit \(host.title)")
        }
        .padding(.leading, 14)
        .padding(.trailing, 8)
        .frame(minHeight: 56)
        .overlay(alignment: .bottom) { Rectangle().fill(Palette.cardDivider).frame(height: 1) }
        .contextMenu {
            Button("Edit…", action: edit)
            Button("Delete", role: .destructive) { model.hosts.delete(host.id) }
        }
    }

    /// `user@host`, with the port only when it is not SSH's 22, as the mock.
    private var address: String {
        let base = "\(host.target.user)@\(host.target.host)"
        return host.target.port == 22 ? base : "\(base):\(host.target.port)"
    }
}
