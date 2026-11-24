package tech.xdge.muxa.app.next

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
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
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.input.TextFieldValue
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

/** A Workspace's Bots (ADR 0082), with how their items ended from the Inbox's [counts]. [talk] starts a talk with one. */
@Composable
fun BotSettings(editor: SettingsEditor, ws: SettingsWorkspace, counts: List<BotCount>, talk: BotTalk) {
    var editing by remember { mutableStateOf<BotDraft?>(null) }
    var settingUp by remember { mutableStateOf(false) }
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
        CardDivider()
        ActionRow("Set up a Bot by talking", "settings.bot.talk", true) { settingUp = true }
    }
    SettingsNote("A Bot owns Routines. Each run starts from its Soul and Memory, and adds what it finds to the Inbox.")
    editing?.let { BotDialog(editor, ws, talk, it) { editing = null } }
    if (settingUp) TalkSetupDialog(talk) { settingUp = false }
}

/** "2 Routines · 4 done · 1 deleted"; the counts only when the Inbox knows them. */
fun botSummary(b: SettingsBot, ws: SettingsWorkspace, counts: List<BotCount>): String {
    val routines = ws.routines.count { it.bot == b.name }
    val count = counts.firstOrNull { it.workspace == ws.name && it.bot == b.name }
    return listOfNotNull(
        if (routines == 1) "1 Routine" else "$routines Routines",
        b.members.takeIf { it.isNotEmpty() }?.let { "Leads " + it.joinToString(", ") },
        count?.let { "${it.done} done · ${it.deleted} deleted" },
    ).joinToString(" · ")
}

@Composable
private fun BotDialog(
    editor: SettingsEditor,
    ws: SettingsWorkspace,
    talk: BotTalk,
    initial: BotDraft,
    close: () -> Unit,
) {
    val workspace = ws.name
    var d by remember { mutableStateOf(initial) }
    var asking by remember { mutableStateOf<BotAsk?>(null) }
    var error by remember { mutableStateOf<String?>(null) }
    val scope = rememberCoroutineScope()
    // The host's copy, so a restored memory shows at once.
    val live = editor.settings?.workspaces?.firstOrNull { it.name == workspace }?.bots?.firstOrNull { it.name == d.name }
    FormDialog(if (d.isNew) "New Bot" else d.name, "settings.bot.form", d.problem == null, { editor.save(listOf(d.op(workspace))) }, close) {
        if (d.isNew) {
            Field(d.name, { d = d.copy(name = it) }, "Name", "bot.name", placeholder = "pr-watch", help = "Lowercase letters, digits, - and _.")
        } else {
            BotRename(editor, workspace, d.name, ws.bots.map { it.name }) { d = d.copy(name = it) }
        }
        Field(d.description, { d = d.copy(description = it) }, "Description", "bot.description", placeholder = "Watches open PRs")
        SwitchField("Enabled", "bot.enabled", d.enabled) { d = d.copy(enabled = it) }
        // ADR 0087: no worker; a Bot has its Workspace's connections and MCP servers.
        Text("Runs with $workspace's access.", Modifier.testTag("bot.access"), style = MaterialTheme.typography.bodySmall)
        BotTeam(d, ws.bots) { d = it }
        Lines(d.commands, "Commands it may run", "bot.command", true, "/path/collect.sh delivery:*", BotDraft::commandProblem) {
            d = d.copy(commands = it)
        }
        Text(
            "Shell commands only this Bot may run, such as /path/collect.sh delivery:*. In its Routines, any other command is refused; " +
                "it tells you what it needed.",
            style = MaterialTheme.typography.bodySmall,
        )
        if (!d.isNew && d.originalSoul == null) {
            Text("soul.md could not be read. Typing here replaces it.", style = MaterialTheme.typography.bodySmall)
        }
        Field(d.soul, { d = d.copy(soul = it) }, "Soul", "bot.soul", mono = true, lines = SOUL_LINES,
            help = "Who the Bot is: its purpose, its scope and how it writes. At most 12 KiB.")
        d.problem?.takeIf { d.name.isNotBlank() }?.let { Text(it, Modifier.testTag("bot.problem"), color = Theme.failure, fontSize = 13.sp) }
        if (live != null) BotMemory(live, editor.readOnly) { asking = BotAsk.RESTORE }
        error?.let { Text(it, Modifier.testTag("bot.error"), color = Theme.failure) }
        if (!d.isNew) {
            SecondaryButton("Talk to ${d.name}", {
                scope.launch {
                    error = talk.start(d.name)
                    if (error == null) close()
                }
            }, Modifier.testTag("bot.talk"))
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

/**
 * A Bot's name with Rename (ADR 0089): one `bot-rename` moves its Routines, teams, folder and items.
 * [renamed] gets the new name, so the editor goes on with it.
 */
@Composable
fun BotRename(editor: SettingsEditor, workspace: String, name: String, names: List<String>, renamed: (String) -> Unit) {
    var field by remember(name) { mutableStateOf(TextFieldValue(name)) }
    var error by remember { mutableStateOf<String?>(null) }
    val scope = rememberCoroutineScope()
    val typed = BotDraft.typedName(field.text)
    BotNameField(field, { field = it }, "bot.name", BotDraft.NAME_RULE)
    SecondaryButton("Rename", {
        val to = typed
        scope.launch {
            error = editor.save(listOf(SettingsOp.BotRename(workspace, name, to)))
            if (error == null) renamed(to)
        }
    }, Modifier.testTag("bot.rename"), enabled = !editor.readOnly && BotDraft.canRename(name, typed, names))
    error?.let { Text(it, Modifier.testTag("bot.rename.error"), color = Theme.failure, fontSize = 13.sp) }
}

/**
 * A Bot's name as typed: lowercase, with spaces as `-`. The text is rewritten only between words: a
 * rewrite while the keyboard composes a word makes it drop letters. Read the name with [BotDraft.typedName].
 */
@Composable
fun BotNameField(value: TextFieldValue, change: (TextFieldValue) -> Unit, tag: String, help: String?, placeholder: String? = null) {
    OutlinedTextField(
        value, { change(if (it.composition == null) it.copy(text = BotDraft.typedName(it.text)) else it) },
        Modifier.fillMaxWidth().testTag(tag),
        label = { Text("Name") },
        placeholder = placeholder?.let { { Text(it) } },
        supportingText = help?.let { { Text(it) } },
        singleLine = true,
        keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.None, autoCorrectEnabled = false),
    )
}

/**
 * The Bots this one leads (ADR 0085): others of the Workspace that lead no team. Hidden for a Bot
 * that is a member of another lead, and when there is no other Bot.
 */
@Composable
fun BotTeam(d: BotDraft, bots: List<SettingsBot>, change: (BotDraft) -> Unit) {
    val choices = BotDraft.memberChoices(d.name.trim(), bots)
    if (choices.isEmpty()) return
    NameChips("Team", choices, d.members, "bot.member") { change(d.copy(members = it)) }
    Text("A team lead hands work to its members. None picked: no team.", style = MaterialTheme.typography.bodySmall)
    if (d.members.isNotEmpty()) {
        Field(d.handoffs, { change(d.copy(handoffs = it)) }, "Handoffs a day", "bot.handoffs", number = true,
            placeholder = "$DEFAULT_HANDOFFS", help = "For all members together, from 1 to 50.")
    }
}

/** What the Bot learned, as its last run wrote it. Only its runs change it; a kept version can come back. */
@Composable
private fun BotMemory(bot: SettingsBot, readOnly: Boolean, restore: () -> Unit) {
    MemoryText(bot.memoryText, "bot.memory")
    if (bot.memoryVersions > 0) {
        SecondaryButton("Restore Earlier Memory", restore, Modifier.testTag("bot.memory.restore"), enabled = !readOnly)
    }
}

/** A `memory.md` under its title, read only: only the runs that keep it write it. */
@Composable
fun MemoryText(text: String?, tag: String) {
    Text("Memory", style = MaterialTheme.typography.titleSmall)
    SelectionContainer(
        Modifier.fillMaxWidth().background(Palette.surface, MEMORY_SHAPE).border(1.dp, Palette.border, MEMORY_SHAPE).padding(10.dp),
    ) {
        Text(text?.takeIf { it.isNotBlank() } ?: "No memory yet.", Modifier.testTag(tag), fontFamily = FontFamily.Monospace, fontSize = 12.5.sp)
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
private const val DEFAULT_HANDOFFS = 10
private const val SOUL_LINES = 8
