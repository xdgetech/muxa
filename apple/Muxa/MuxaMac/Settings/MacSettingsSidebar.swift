import MuxaV2
import SwiftUI

/// The left column of Settings: the host's Workspaces, then the host's
/// workers and connection, then what belongs to this Mac.
struct MacSettingsSidebar: View {
    @Environment(NextModel.self) private var model
    let current: MacSettingsPlace
    let pick: (MacSettingsPlace) -> Void
    @State private var addingWorkspace = false

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 2) {
                label("Workspaces", top: 6)
                ForEach(model.settings.settings?.workspaces ?? []) { w in
                    row(w.name, meta: Self.meta(w), place: .workspace(w.name), id: "settings.workspace.\(w.name)")
                }
                Button { addingWorkspace = true } label: {
                    Text("+ New Workspace")
                        .foregroundStyle(Palette.accent)
                        .padding(.horizontal, 10)
                        .frame(maxWidth: .infinity, minHeight: 36, alignment: .leading)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .disabled(model.settings.settings == nil || model.settings.readOnly)
                .accessibilityIdentifier("settings.addworkspace")
                label("Host", top: 16)
                if model.settings.settings != nil {
                    row("Workers", meta: "\(model.settings.settings?.workers.count ?? 0)", place: .workers, id: "settings.workers")
                    row("Capture assistant", meta: model.settings.settings?.assistant?.enabled == true ? "on" : "off",
                        place: .assistant, id: "settings.assistant")
                }
                row(model.connectionTitle, meta: status, place: .connection, id: "settings.connection")
                label("This Mac", top: 16)
                row("Keyboard", meta: Shortcut.shortcuts.display, place: .keyboard, id: "settings.keyboard")
                row("Appearance", meta: ThemeStore.shared.theme.name, place: .terminal, id: "settings.terminal")
            }
            .padding(.horizontal, 10)
            .padding(.vertical, 14)
        }
        .sheet(isPresented: $addingWorkspace) { WorkspaceAddSheet() }
    }

    private func label(_ title: String, top: CGFloat) -> some View {
        SectionLabel(title: title)
            .padding(.horizontal, 10)
            .padding(.top, top)
            .padding(.bottom, 4)
    }

    private func row(_ title: String, meta: String, place: MacSettingsPlace, id: String) -> some View {
        Button { pick(place) } label: {
            HStack(spacing: 8) {
                Text(title).font(.system(size: 13.5)).lineLimit(1)
                Spacer(minLength: 4)
                Text(meta).font(.system(size: 12)).foregroundStyle(Palette.textTertiary).lineLimit(1)
            }
            .foregroundStyle(current == place ? Palette.text : Palette.textMid)
            .padding(.horizontal, 10)
            .frame(height: 36)
            .selectableRow(current == place)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityElement(children: .combine)
        .accessibilityAddTraits(current == place ? .isSelected : [])
        .accessibilityIdentifier(id)
    }

    private var status: String {
        switch model.link {
        case .connected: "connected"
        case .connecting: "connecting"
        case .reconnecting: "reconnecting"
        case .failed: "failed"
        case .offline: "offline"
        }
    }

    static func meta(_ w: SettingsWorkspace) -> String {
        w.connections.count == 1 ? "1 connection" : "\(w.connections.count) connections"
    }
}
