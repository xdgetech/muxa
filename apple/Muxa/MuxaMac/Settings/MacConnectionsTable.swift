import MuxaV2
import SwiftUI

/// The Connections tab: the note, then one row per connection with its
/// secret's state, account, access, who uses it and when it was last used.
struct MacConnectionsTable: View {
    @Environment(NextModel.self) private var model
    let workspace: SettingsWorkspace
    @State private var editing: ConnectionDraft?

    private enum Width {
        static let name: CGFloat = 170
        static let account: CGFloat = 150
        static let access: CGFloat = 110
        static let last: CGFloat = 90
        static let edit: CGFloat = 60
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            MacInfoNote(text: "Every agent in this Workspace, Task or Routine, gets all of these as environment variables. "
                + "Tokens stay in the Keychain and never come back to the apps.")
                .accessibilityIdentifier("ws.note")
            VStack(spacing: 0) {
                MacTableHeader(columns: [
                    ("Connection", Width.name), ("Account", Width.account), ("Access", Width.access),
                    ("Used by", nil), ("Last used", Width.last), ("", Width.edit),
                ])
                ForEach(workspace.connections) { row($0) }
            }
            MacAddButton(title: "+ Add connection", id: "connection.add") { editing = ConnectionDraft() }
        }
        .sheet(item: $editing) { ConnectionEditor(workspace: workspace.name, draft: $0) }
    }

    private func row(_ c: SettingsConnection) -> some View {
        let used = model.settings.usage[workspace.name]?.connections.first { $0.name == c.name }
        let usedBy = Self.usedBy(used?.usedBy)
        let last = Self.lastUsed(used?.lastUsed)
        return Button { editing = ConnectionDraft(c) } label: {
            MacTableRow {
                HStack(spacing: 8) {
                    Circle().fill(c.secret.color).frame(width: 8, height: 8)
                    VStack(alignment: .leading, spacing: 1) {
                        Text(c.name).fontWeight(.medium).lineLimit(1)
                        Text(c.spec.env).font(.system(size: 10.5, design: .monospaced))
                            .foregroundStyle(Palette.textTertiary).lineLimit(1)
                    }
                }
                .cell(Width.name)
                Text(c.spec.account ?? "—").foregroundStyle(Palette.textMid).lineLimit(1).cell(Width.account)
                access(c.spec.access).cell(Width.access)
                Text(usedBy).foregroundStyle(Palette.textSecondary).lineLimit(1).truncationMode(.tail).cell(nil)
                Text(last).foregroundStyle(Palette.textTertiary).lineLimit(1).cell(Width.last)
                MacEditChip().cell(Width.edit)
            }
        }
        .buttonStyle(.plain)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel([c.name, c.spec.env, c.spec.account ?? "", c.spec.access ?? "", c.secret.rawValue, usedBy, last]
            .filter { !$0.isEmpty }.joined(separator: ", "))
        .accessibilityAddTraits(.isButton)
        .accessibilityIdentifier("connection.\(c.name)")
    }

    @ViewBuilder
    private func access(_ access: String?) -> some View {
        if let access {
            let writes = Self.writes(access)
            Badge(text: access, foreground: writes ? Palette.writeText : Palette.readText,
                  background: writes ? Palette.writeBack : Palette.readBack)
        } else {
            Text("—").foregroundStyle(Palette.textDim)
        }
    }

    /// Access that changes outside systems gets the write tones.
    static func writes(_ access: String) -> Bool {
        let a = access.lowercased()
        return a.contains("write") || a.contains("create")
    }

    static func usedBy(_ usedBy: UsedBy?) -> String {
        var who = usedBy?.routines ?? []
        if let t = usedBy?.tasks.count, t > 0 { who.append(t == 1 ? "1 Task" : "\(t) Tasks") }
        return who.isEmpty ? "No one now" : who.joined(separator: ", ")
    }

    static func lastUsed(_ seconds: Int64?) -> String {
        guard let seconds else { return "never" }
        let f = RelativeDateTimeFormatter()
        f.unitsStyle = .abbreviated
        return f.localizedString(for: Date(timeIntervalSince1970: TimeInterval(seconds)), relativeTo: .now)
    }
}

/// The mock's blue note with a lock: what every agent gets.
struct MacInfoNote: View {
    let text: String

    var body: some View {
        HStack(spacing: 12) {
            Image(systemName: "lock").font(.system(size: 13, weight: .semibold))
            Text(text).lineSpacing(3).frame(maxWidth: .infinity, alignment: .leading)
        }
        .foregroundStyle(Palette.infoText)
        .padding(.horizontal, 14)
        .padding(.vertical, 10)
        .background(Palette.infoBack, in: RoundedRectangle(cornerRadius: 10))
        .overlay(RoundedRectangle(cornerRadius: 10).strokeBorder(Palette.infoBorder))
        .accessibilityElement(children: .combine)
    }
}
