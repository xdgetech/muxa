package tech.xdge.muxa.app.next

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.padding
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.unit.dp
import tech.xdge.muxa.v2.McpSpec
import tech.xdge.muxa.v2.SettingsOp
import tech.xdge.muxa.v2.SettingsWorkspace
import tech.xdge.muxa.v2.Workspace

/** A Workspace's MCP servers. Listing one makes MCP strict for its Claude Code agents (ADR 0075). */
@Composable
fun McpSettings(editor: SettingsEditor, ws: SettingsWorkspace, usage: Workspace?, model: NextModel) {
    var editing by remember { mutableStateOf<McpDraft?>(null) }
    WorkspaceToolsNote(ws)
    SettingsHeader("MCP servers")
    SettingsCard {
        if (ws.mcp.isEmpty()) SettingsNote("No MCP servers: agents use their own.")
        for (m in ws.mcp) {
            val used = usage?.mcp?.firstOrNull { it.name == m.name }
            EntryRow("settings.mcp.${ws.name}.${m.name}", !editor.readOnly, { editing = McpDraft.of(m) }, badge = m.name.take(2).uppercase()) {
                EntryTitle("${m.name} · ${if (m.url != null) m.type ?: McpDraft.HTTP else "command"}")
                EntryDetail(detail(m), mono = true)
                if (m.connections.isNotEmpty()) EntryDetail("Needs " + m.connections.joinToString(", "))
                UsageLines(used?.usedBy, used?.lastUsed, model)
            }
            CardDivider()
        }
        if (ws.mcp.isNotEmpty()) {
            val strict = if (ws.claudeConnectors) {
                "Agents in this Workspace get these MCP servers and your claude.ai connectors."
            } else {
                "Strict: agents in this Workspace get only these MCP servers."
            }
            SettingsNote(strict, Modifier.testTag("settings.strict.${ws.name}"))
        }
        ActionRow("+ Add MCP server", "settings.mcp.add", !editor.readOnly) { editing = McpDraft(isNew = true) }
    }
    ClaudeConnectors(editor, ws)
    BotRulesBlock(editor, ws)
    editing?.let { McpDialog(editor, ws, it) { editing = null } }
}

/** ADR 0087: the user's claude.ai connectors may load beside the Workspace's MCP servers. */
@Composable
private fun ClaudeConnectors(editor: SettingsEditor, ws: SettingsWorkspace) {
    val save = rememberInlineSave(editor)
    SettingsCard {
        Column(Modifier.padding(horizontal = 14.dp, vertical = 6.dp)) {
            SwitchField("Allow claude.ai connectors", "settings.claudeconnectors", ws.claudeConnectors, !editor.readOnly) {
                save.run(listOf(SettingsOp.WorkspaceUpdate(ws.name, claudeConnectors = it)))
            }
        }
    }
    SettingsNote("Agents here also get your claude.ai connectors, such as Calendar or Sheets, beside the MCP servers above.")
    save.Error()
}

/** `[workspace.bots]` (ADR 0087): each list saves on its own, and the other list goes with it unchanged. */
@Composable
private fun BotRulesBlock(editor: SettingsEditor, ws: SettingsWorkspace) {
    val save = rememberInlineSave(editor)
    val on = !editor.readOnly
    val rules = ws.botRules
    SettingsHeader("Bot rules")
    SettingsCard {
        Column(Modifier.padding(14.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Lines(rules.allow, "Allowed", "settings.botrules.allow", on, "WebSearch") {
                save.run(listOf(SettingsOp.WorkspaceUpdate(ws.name, botRules = rules.copy(allow = it))))
            }
            Lines(rules.deny, "Denied", "settings.botrules.deny", on, "Bash") {
                save.run(listOf(SettingsOp.WorkspaceUpdate(ws.name, botRules = rules.copy(deny = it))))
            }
        }
    }
    SettingsNote("Claude Code permission rules, such as Bash or mcp__claude_ai_Gmail. Only this Workspace's Bots get them.")
    save.Error()
}

private fun detail(m: McpSpec): String = m.url ?: (listOfNotNull(m.command) + m.args.orEmpty()).joinToString(" ")

@Composable
private fun McpDialog(editor: SettingsEditor, ws: SettingsWorkspace, initial: McpDraft, close: () -> Unit) {
    var d by remember { mutableStateOf(initial) }
    FormDialog(
        if (d.isNew) "New MCP Server" else d.name, "settings.mcp.form", d.problem == null, { editor.save(listOf(d.op(ws.name))) }, close,
        remove = if (d.isNew) null else ({ editor.save(listOf(SettingsOp.McpRemove(ws.name, d.name))) }),
        removeTitle = "Remove MCP Server",
    ) {
        Field(d.name, { d = d.copy(name = it) }, "Name", "mcp.name", enabled = d.isNew, placeholder = "docs")
        Choice(listOf(false, true), listOf("Command", "URL"), d.remote, "mcp.kind") { d = d.copy(remote = it) }
        if (d.remote) {
            Field(d.url, { d = d.copy(url = it) }, "URL", "mcp.url", mono = true, placeholder = "https://mcp.example.com/mcp")
            Choice(listOf(McpDraft.HTTP, McpDraft.SSE), listOf("HTTP", "SSE"), d.type, "mcp.type") { d = d.copy(type = it) }
            Field(d.headers, { d = d.copy(headers = it) }, "Headers", "mcp.headers", mono = true, lines = 2, help = "Name: value, one per line.")
        } else {
            Field(d.command, { d = d.copy(command = it) }, "Command", "mcp.command", mono = true, placeholder = "npx")
            Field(d.args, { d = d.copy(args = it) }, "Args", "mcp.args", mono = true, lines = 2, help = "One per line.")
            Field(d.env, { d = d.copy(env = it) }, "Env", "mcp.env", mono = true, lines = 2, help = "KEY=value, one per line.")
        }
        NameChips("Connections it needs", ws.connectionNames(), d.connections, "mcp.connection") { d = d.copy(connections = it) }
        Text("Nothing here is a secret: a token belongs in a connection.", style = MaterialTheme.typography.bodySmall)
        d.problem?.let { Text(it, style = MaterialTheme.typography.bodySmall) }
    }
}
