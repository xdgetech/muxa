import MuxaV2
import SwiftUI

/// The Bots tab (ADR 0082): the Workspace's Bots with their Routines and how
/// their items ended. Editing opens the shared Bot editor.
struct MacBotsTable: View {
    @Environment(NextModel.self) private var model
    let workspace: SettingsWorkspace
    @State private var editing: BotDraft?

    private enum Width {
        static let name: CGFloat = 200
        static let edit: CGFloat = 60
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            Text("A Bot owns Routines. Each run starts from its soul and memory, and puts at most its daily items in the Inbox.")
                .font(.system(size: 12))
                .foregroundStyle(Palette.textTertiary)
                .fixedSize(horizontal: false, vertical: true)
            VStack(spacing: 0) {
                MacTableHeader(columns: [("Bot", Width.name), ("Routines and items", nil), ("", Width.edit)])
                ForEach(workspace.bots) { row($0) }
            }
            MacAddButton(title: "+ Add Bot", id: "bot.add") { editing = BotDraft() }
        }
        .sheet(item: $editing) { BotEditor(workspace: workspace, draft: $0) }
    }

    private func row(_ b: SettingsBot) -> some View {
        let summary = BotDraft.summary(b, workspace: workspace, inbox: model.inbox)
        return Button { editing = BotDraft(b) } label: {
            MacTableRow {
                HStack(spacing: 8) {
                    Text(b.name).fontWeight(.medium).lineLimit(1)
                    if !b.spec.enabled { Badge(text: "off") }
                }
                .cell(Width.name)
                Text(summary).foregroundStyle(Palette.textMid).lineLimit(1).cell(nil)
                MacEditChip().cell(Width.edit)
            }
        }
        .buttonStyle(.plain)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("\(b.name), \(summary)" + (b.spec.enabled ? "" : ", off"))
        .accessibilityAddTraits(.isButton)
        .accessibilityIdentifier("settings.bot.\(b.name)")
    }
}
