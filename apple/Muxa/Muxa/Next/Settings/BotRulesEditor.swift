import MuxaV2
import SwiftUI

/// `[workspace.bots]` (ADR 0087): the Claude Code permission rules only this
/// Workspace's Bots get, as two editable lists saved together.
struct BotRulesEditor: View {
    @Environment(NextModel.self) private var model
    let workspace: SettingsWorkspace
    @State private var allow: [EditableLine]
    @State private var deny: [EditableLine]
    @State private var error: String?

    static let note = "Claude Code permission rules, such as Bash or mcp__claude_ai_Gmail. Only this Workspace's Bots get them."

    init(workspace: SettingsWorkspace) {
        self.workspace = workspace
        _allow = State(initialValue: workspace.botRules.allow.map(EditableLine.init))
        _deny = State(initialValue: workspace.botRules.deny.map(EditableLine.init))
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            list("Allowed", lines: $allow, id: "allow")
            list("Denied", lines: $deny, id: "deny")
            HStack {
                Button("Save Bot Rules") { Task { await save() } }
                    .disabled(rules == workspace.botRules || model.settings.saving || model.settings.readOnly)
                    .accessibilityIdentifier("ws.botrules.save")
                Spacer()
            }
            SaveError(text: error, id: "ws.botrules.error")
        }
        .onChange(of: workspace.botRules) { _, saved in
            allow = saved.allow.map(EditableLine.init)
            deny = saved.deny.map(EditableLine.init)
        }
    }

    /// The lists as the host takes them: trimmed, without empty lines.
    private var rules: BotRules {
        func clean(_ lines: [EditableLine]) -> [String] { lines.map { Lines.trimmed($0.text) }.filter { !$0.isEmpty } }
        return BotRules(allow: clean(allow), deny: clean(deny))
    }

    private func list(_ title: String, lines: Binding<[EditableLine]>, id: String) -> some View {
        EditableLineList(title: title, lines: lines, placeholder: "Rule, e.g. WebSearch",
                         addTitle: "Add \(title == "Allowed" ? "an allow" : "a deny") rule", id: "ws.botrules.\(id)")
            .disabled(model.settings.readOnly)
    }

    private func save() async {
        error = await model.settings.save([.workspaceUpdate(workspace: workspace.name, botRules: rules)])
    }
}
