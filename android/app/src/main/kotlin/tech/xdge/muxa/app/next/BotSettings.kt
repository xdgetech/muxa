package tech.xdge.muxa.app.next

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlinx.coroutines.launch
import tech.xdge.muxa.app.ui.DestructiveButton
import tech.xdge.muxa.app.ui.KindPalette
import tech.xdge.muxa.app.ui.Palette
import tech.xdge.muxa.app.ui.SecondaryButton
import tech.xdge.muxa.app.ui.Theme
import tech.xdge.muxa.v2.BotCount
import tech.xdge.muxa.v2.SettingsBot
import tech.xdge.muxa.v2.SettingsOp
import tech.xdge.muxa.v2.SettingsWorkspace

/** A Workspace's Bots (ADR 0082), with how their items ended from the Inbox's [counts]. */
@Composable
fun BotSettings(editor: SettingsEditor, ws: SettingsWorkspace, counts: List<BotCount>) {
    var editing by remember { mutableStateOf<BotDraft?>(null) }
    SettingsHeader("Bots")
    SettingsCard {
        if (ws.bots.isEmpty()) SettingsNote("No Bots.")
        for (b in ws.bots) {
            EntryRow("settings.bot.${ws.name}.${b.name}", !editor.readOnly, { editing = BotDraft.of(b) }, badge = null) {
                EntryTitle(b.name) { if (!b.enabled) Chip("off", KindPalette.todo) }
                b.description?.let { EntryDetail(it) }
                EntryDetail(botSummary(b, ws, counts))
            }
            CardDivider()
        }
        ActionRow("+ Add Bot", "settings.bot.add", !editor.readOnly) { editing = BotDraft(isNew = true) }
    }
    SettingsNote("A Bot owns Routines. Each run starts from its Soul and Memory, and adds at most its daily items to the Inbox.")
    val workers = editor.settings?.workers.orEmpty().map { it.name }
    editing?.let { BotDialog(editor, ws.name, workers, it) { editing = null } }
}

/** "2 Routines · 3 a day · 4 done · 1 deleted"; the counts only when the Inbox knows them. */
fun botSummary(b: SettingsBot, ws: SettingsWorkspace, counts: List<BotCount>): String {
    val routines = ws.routines.count { it.bot == b.name }
    val count = counts.firstOrNull { it.workspace == ws.name && it.bot == b.name }
    return listOfNotNull(
        if (routines == 1) "1 Routine" else "$routines Routines",
        "${b.dailyItems ?: DEFAULT_DAILY_ITEMS} a day",
        b.worker?.let { "worker $it" },
        count?.let { "${it.done} done · ${it.deleted} deleted" },
    ).joinToString(" · ")
}

@Composable
private fun BotDialog(editor: SettingsEditor, workspace: String, workers: List<String>, initial: BotDraft, close: () -> Unit) {
    var d by remember { mutableStateOf(initial) }
    var asking by remember { mutableStateOf<BotAsk?>(null) }
    var error by remember { mutableStateOf<String?>(null) }
    val scope = rememberCoroutineScope()
    // The host's copy, so a restored memory shows at once.
    val live = editor.settings?.workspaces?.firstOrNull { it.name == workspace }?.bots?.firstOrNull { it.name == d.name }
    FormDialog(if (d.isNew) "New Bot" else d.name, "settings.bot.form", d.problem == null, { editor.save(listOf(d.op(workspace))) }, close) {
        Field(d.name, { d = d.copy(name = it) }, "Name", "bot.name", enabled = d.isNew, placeholder = "pr-watch",
            help = if (d.isNew) "Lowercase letters, digits, - and _." else null)
        Field(d.description, { d = d.copy(description = it) }, "Description", "bot.description", placeholder = "Watches open PRs")
        NamePicker("Worker", "bot.worker", workers, d.worker) { d = d.copy(worker = it) }
        Field(d.dailyItems, { d = d.copy(dailyItems = it) }, "Daily items", "bot.daily", number = true,
            placeholder = "$DEFAULT_DAILY_ITEMS", help = "At most this many Inbox items a day, from 1 to 50.")
        SwitchField("Enabled", "bot.enabled", d.enabled) { d = d.copy(enabled = it) }
        if (!d.isNew && d.originalSoul == null) {
            Text("soul.md could not be read. Typing here replaces it.", style = MaterialTheme.typography.bodySmall)
        }
        Field(d.soul, { d = d.copy(soul = it) }, "Soul", "bot.soul", mono = true, lines = SOUL_LINES,
            help = "Who the Bot is: its purpose, its scope and how it writes. At most 12 KiB.")
        d.problem?.takeIf { d.name.isNotBlank() }?.let { Text(it, Modifier.testTag("bot.problem"), color = Theme.failure, fontSize = 13.sp) }
        if (live != null) BotMemory(live, editor.readOnly) { asking = BotAsk.RESTORE }
        error?.let { Text(it, Modifier.testTag("bot.error"), color = Theme.failure) }
        if (!d.isNew) {
            DestructiveButton("Remove Bot", { asking = BotAsk.REMOVE }, Modifier.testTag("bot.remove"), enabled = !editor.readOnly)
        }
    }
    asking?.let { ask ->
        val op = when (ask) {
            BotAsk.RESTORE -> SettingsOp.BotMemoryRestore(workspace, d.name, version = 1)
            BotAsk.REMOVE -> SettingsOp.BotRemove(workspace, d.name)
        }
        ConfirmDialog(ask.title(d.name), ask.text, ask.action, {
            asking = null
            scope.launch {
                error = editor.save(listOf(op))
                if (error == null && ask == BotAsk.REMOVE) close()
            }
        }) { asking = null }
    }
}

/** What the Bot learned, as its last run wrote it. Only its runs change it; a kept version can come back. */
@Composable
private fun BotMemory(bot: SettingsBot, readOnly: Boolean, restore: () -> Unit) {
    Text("Memory", style = MaterialTheme.typography.titleSmall)
    SelectionContainer(
        Modifier.fillMaxWidth().background(Palette.surface, MEMORY_SHAPE).border(1.dp, Palette.border, MEMORY_SHAPE).padding(10.dp),
    ) {
        Text(
            bot.memoryText?.takeIf { it.isNotBlank() } ?: "No memory yet.",
            Modifier.testTag("bot.memory"),
            fontFamily = FontFamily.Monospace,
            fontSize = 12.5.sp,
        )
    }
    if (bot.memoryVersions > 0) {
        SecondaryButton("Restore Earlier Memory", restore, Modifier.testTag("bot.memory.restore"), enabled = !readOnly)
    }
}

private enum class BotAsk(val action: String, val text: String) {
    RESTORE("Restore", "The memory before the last run's comes back. The current one is kept as an earlier version."),
    REMOVE("Remove", "muxa forgets this Bot. Its Soul and Memory files stay in .muxa/bots/. The host refuses while Routines name it."),
    ;

    fun title(bot: String) = when (this) {
        RESTORE -> "Restore $bot's earlier memory?"
        REMOVE -> "Remove $bot?"
    }
}

private val MEMORY_SHAPE = RoundedCornerShape(8.dp)
private const val DEFAULT_DAILY_ITEMS = 3
private const val SOUL_LINES = 8
