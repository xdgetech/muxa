import MuxaV2
import SwiftUI

/// The MCP servers tab: one card per server with its kind, command or URL,
/// and who uses it. The mock's tools badge is left out: the host does not say.
struct MacMcpCards: View {
    @Environment(NextModel.self) private var model
    let workspace: SettingsWorkspace
    @State private var editing: McpDraft?

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            Text("Every agent in this Workspace gets these MCP servers. muxa writes them into the agent's config "
                + "(Claude Code and Codex) when it starts. Listing any MCP server makes MCP strict: "
                + "Claude Code then loads only these servers, unless claude.ai connectors are allowed below.")
                .foregroundStyle(Palette.textSecondary)
                .lineSpacing(4)
                .fixedSize(horizontal: false, vertical: true)
            ForEach(workspace.mcp) { card($0) }
            MacAddButton(title: "+ Add MCP server", id: "mcp.add") { editing = McpDraft() }
            VStack(alignment: .leading, spacing: 6) {
                ClaudeConnectorsToggle(workspace: workspace).toggleStyle(.switch)
                Text(ClaudeConnectorsToggle.note)
                    .font(.system(size: 12))
                    .foregroundStyle(Palette.textTertiary)
                    .fixedSize(horizontal: false, vertical: true)
            }
            .padding(14)
            .raisedCard()
            .padding(.top, 6)
            VStack(alignment: .leading, spacing: 8) {
                Text("Bot rules").fontWeight(.semibold)
                Text(BotRulesEditor.note)
                    .font(.system(size: 12))
                    .foregroundStyle(Palette.textTertiary)
                    .fixedSize(horizontal: false, vertical: true)
                BotRulesEditor(workspace: workspace).textFieldStyle(.roundedBorder)
            }
            .padding(14)
            .frame(maxWidth: 640, alignment: .leading)
            .raisedCard()
        }
        .sheet(item: $editing) { McpEditor(workspace: workspace, draft: $0) }
    }

    private func card(_ m: McpSpec) -> some View {
        let used = model.settings.usage[workspace.name]?.mcp.first { $0.name == m.name }
        let usage = UsageLine.text(usedBy: used?.usedBy, lastUsed: used?.lastUsed)
        let detail = WorkspaceMcpForm.detail(m)
        let kind = Self.kind(m)
        return Button { editing = McpDraft(m) } label: {
            HStack(spacing: 14) {
                Text(Self.short(m.name))
                    .font(.system(size: 12, weight: .semibold, design: .monospaced))
                    .foregroundStyle(Palette.textMid)
                    .frame(width: 34, height: 34)
                    .background(Palette.rowSelected, in: RoundedRectangle(cornerRadius: 8))
                VStack(alignment: .leading, spacing: 3) {
                    HStack(spacing: 4) {
                        Text(m.name).fontWeight(.semibold)
                        Text("· \(kind)").foregroundStyle(Palette.textTertiary)
                    }
                    Text(detail).font(.system(size: 12)).foregroundStyle(Palette.textTertiary)
                        .lineLimit(1).truncationMode(.tail)
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                Text(usage).font(.system(size: 12)).foregroundStyle(Palette.textSecondary)
                    .lineLimit(2).frame(width: 260, alignment: .leading)
                MacEditChip().frame(width: 60)
            }
            .padding(.horizontal, 14)
            .padding(.vertical, 12)
            .raisedCard()
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("\(m.name), \(kind), \(detail), \(usage)")
        .accessibilityAddTraits(.isButton)
        .accessibilityIdentifier("mcp.\(m.name)")
    }

    static func kind(_ m: McpSpec) -> String {
        m.url == nil ? "command" : (m.transport ?? "http")
    }

    /// The tile's two letters: the name's first two letters or digits.
    static func short(_ name: String) -> String {
        String(name.filter { $0.isLetter || $0.isNumber }.prefix(2)).uppercased()
    }
}
