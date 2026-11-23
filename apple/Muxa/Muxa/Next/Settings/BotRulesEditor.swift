import MuxaV2
import SwiftUI

/// `[workspace.bots]` (ADR 0087): the Claude Code permission rules only this
/// Workspace's Bots get, as two editable lists saved together.
struct BotRulesEditor: View {
    @Environment(NextModel.self) private var model
    let workspace: SettingsWorkspace
    @State private var allow: [RuleLine]
    @State private var deny: [RuleLine]
    @State private var error: String?

    static let note = "Claude Code permission rules, such as Bash or mcp__claude_ai_Gmail. Only this Workspace's Bots get them."

    init(workspace: SettingsWorkspace) {
        self.workspace = workspace
        _allow = State(initialValue: workspace.botRules.allow.map(RuleLine.init))
        _deny = State(initialValue: workspace.botRules.deny.map(RuleLine.init))
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
            allow = saved.allow.map(RuleLine.init)
            deny = saved.deny.map(RuleLine.init)
        }
    }

    /// The lists as the host takes them: trimmed, without empty lines.
    private var rules: BotRules {
        func clean(_ lines: [RuleLine]) -> [String] { lines.map { Lines.trimmed($0.text) }.filter { !$0.isEmpty } }
        return BotRules(allow: clean(allow), deny: clean(deny))
    }

    private func list(_ title: String, lines: Binding<[RuleLine]>, id: String) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(title).font(.subheadline.weight(.semibold))
            if lines.wrappedValue.isEmpty {
                Text("none").font(.callout).foregroundStyle(Palette.textTertiary)
            }
            ForEach(lines) { $line in
                HStack(spacing: 8) {
                    TextField("Rule, e.g. WebSearch", text: $line.text)
                        .monospaced()
                        .plainEntry()
                        .accessibilityIdentifier("ws.botrules.\(id).\(index(of: line, in: lines.wrappedValue))")
                    Button("Remove", systemImage: "minus.circle") {
                        lines.wrappedValue.removeAll { $0.id == line.id }
                    }
                    .labelStyle(.iconOnly)
                    .buttonStyle(.borderless)
                    .accessibilityIdentifier("ws.botrules.\(id).remove.\(index(of: line, in: lines.wrappedValue))")
                }
            }
            Button("Add \(title == "Allowed" ? "an allow" : "a deny") rule", systemImage: "plus") {
                lines.wrappedValue.append(RuleLine(""))
            }
            .buttonStyle(.borderless)
            .accessibilityIdentifier("ws.botrules.\(id).add")
        }
        .disabled(model.settings.readOnly)
    }

    private func save() async {
        error = await model.settings.save([.workspaceUpdate(workspace: workspace.name, botRules: rules)])
    }

    private func index(of line: RuleLine, in lines: [RuleLine]) -> Int {
        lines.firstIndex { $0.id == line.id } ?? 0
    }
}

/// One editable rule; its id keeps the row while the text changes.
private struct RuleLine: Identifiable, Equatable {
    let id = UUID()
    var text: String

    init(_ text: String) { self.text = text }
}
