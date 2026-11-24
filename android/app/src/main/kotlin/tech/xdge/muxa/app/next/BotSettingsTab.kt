package tech.xdge.muxa.app.next

import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.sp
import kotlinx.coroutines.launch
import tech.xdge.muxa.app.ui.DestructiveButton
import tech.xdge.muxa.app.ui.PrimaryButton
import tech.xdge.muxa.app.ui.Theme

/**
 * A Bot's Settings on its page: the Bot editor's fields inline, its Workspace's access (ADR 0087), and
 * Remove, which first moves its Routines to another Bot or to none.
 */
@Composable
fun ColumnScope.BotSettingsTab(model: NextModel, editor: SettingsEditor, e: BotEntry, bots: BotNav) {
    val ws = e.workspace
    val scope = rememberCoroutineScope()
    val original = remember(e.bot) { BotDraft.of(e.bot) }
    var d by remember(e.bot) { mutableStateOf(original) }
    var saved by remember { mutableStateOf<String?>(null) }
    val on = !editor.readOnly
    LaunchedEffect(Unit) { model.loadPlanUsage() }
    BotRename(editor, ws.name, e.name, ws.bots.map { it.name }) { bots.pick = BotPick(ws.name, it) }
    Field(d.description, { d = d.copy(description = it) }, "Description", "bot.description", enabled = on, placeholder = "Watches open PRs")
    Field(d.dailyItems, { d = d.copy(dailyItems = it) }, "Daily items", "bot.daily", enabled = on, number = true,
        placeholder = "$DEFAULT_DAILY_ITEMS", help = "At most this many Inbox items a day, from 1 to 50.")
    SwitchField("Enabled", "bot.enabled", d.enabled, on) { d = d.copy(enabled = it) }
    Text("Turned off, its Routines do not run.", style = MaterialTheme.typography.bodySmall)
    BotTeam(d, ws.bots) { d = it }
    Lines(d.commands, "Commands it may run", "bot.command", on, "/path/collect.sh delivery:*", BotDraft::commandProblem) {
        d = d.copy(commands = it)
    }
    Text(
        "Shell commands only this Bot may run, such as /path/collect.sh delivery:*. In its Routines, any other command is refused; " +
            "it tells you what it needed.",
        style = MaterialTheme.typography.bodySmall,
    )
    // A Bot runs on its Workspace's logins only (ADR 0091 §4): a login carries connectors and settings.
    if (ws.subscriptions.isNotEmpty()) {
        Text("Subscriptions", style = MaterialTheme.typography.titleSmall)
        SubscriptionOrder(d.subscriptions, ws.subscriptions, model.planUsage, "bot.subs", on) { d = d.copy(subscriptions = it) }
        Text(
            "Empty: ${ws.name}'s order. To run it on another login, add that login to ${ws.name} first.",
            style = MaterialTheme.typography.bodySmall,
        )
    }
    d.problem?.let { Text(it, Modifier.testTag("bot.problem"), color = Theme.failure, fontSize = 13.sp) }
    PrimaryButton("Save", {
        scope.launch { saved = editor.save(listOf(d.op(ws.name))) }
    }, Modifier.testTag("bot.settings.save"), enabled = on && d != original && d.problem == null)
    saved?.let { Text(it, Modifier.testTag("bot.settings.error"), color = Theme.failure, fontSize = 13.sp) }
    BotAccess(editor, e)
    BotRemove(editor, e, bots)
}

/** What the Bot may use: its Workspace's connections and MCP servers, claude.ai connectors and the Bot rules. */
@Composable
private fun BotAccess(editor: SettingsEditor, e: BotEntry) {
    val ws = e.workspace
    SettingsHeader("Access")
    SettingsNote("${e.name} runs with ${ws.name}'s access. To give it a tool, add it to ${ws.name}; to keep data from it, use another Workspace.")
    SettingsCard {
        EntryRow("bot.access.connections", false, {}, badge = null) {
            EntryTitle("Connections")
            EntryDetail(ws.connections.joinToString(", ") { "${it.name} (${it.secret.wire})" }.ifEmpty { "None" })
        }
        CardDivider()
        EntryRow("bot.access.mcp", false, {}, badge = null) {
            EntryTitle("MCP servers")
            EntryDetail(ws.mcp.joinToString(", ") { it.name }.ifEmpty { "None" })
        }
    }
    ClaudeConnectors(editor, ws)
    BotRulesBlock(editor, ws)
}

/** Remove: its Routines move to [target] first, in the same update, so none is left naming a Bot that is gone. */
@Composable
private fun BotRemove(editor: SettingsEditor, e: BotEntry, bots: BotNav) {
    val ws = e.workspace
    val scope = rememberCoroutineScope()
    var target by remember { mutableStateOf<String?>(null) }
    var asking by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }
    val routines = ws.routines.count { it.bot == e.name }
    SettingsHeader("Remove")
    if (routines > 0) {
        NamePicker("Move its Routines to", "bot.remove.target", ws.bots.map { it.name } - e.name, target) { target = it }
        Text(
            if (target == null) "Its ${count(routines, "Routine")} will have no Bot." else "Its ${count(routines, "Routine")} will belong to $target.",
            style = MaterialTheme.typography.bodySmall,
        )
    }
    DestructiveButton("Remove ${e.name}", { asking = true }, Modifier.testTag("bot.remove"), enabled = !editor.readOnly)
    error?.let { Text(it, Modifier.testTag("bot.remove.error"), color = Theme.failure, fontSize = 13.sp) }
    if (asking) {
        ConfirmDialog(
            "Remove ${e.name}?",
            "muxa forgets this Bot. Its Soul and Memory files stay in .muxa/bots/${e.name}/.",
            "Remove",
            {
                asking = false
                scope.launch {
                    error = editor.save(ws.botRemoval(e.name, target))
                    if (error == null) bots.back()
                }
            },
        ) { asking = false }
    }
}
