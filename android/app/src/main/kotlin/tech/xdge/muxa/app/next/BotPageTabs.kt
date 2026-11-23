package tech.xdge.muxa.app.next

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlinx.coroutines.launch
import tech.xdge.muxa.app.ui.Palette
import tech.xdge.muxa.app.ui.SecondaryButton
import tech.xdge.muxa.app.ui.Theme
import tech.xdge.muxa.v2.MemoryVersion
import tech.xdge.muxa.v2.Routine
import tech.xdge.muxa.v2.SettingsOp
import tech.xdge.muxa.v2.SettingsRoutine
import java.time.Instant
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import java.time.format.FormatStyle

/** A Bot's Routines with their switch, Run now, last run and Edit. New ones usually come from a chat. */
@Composable
fun ColumnScope.BotRoutines(model: NextModel, editor: SettingsEditor, e: BotEntry) {
    val save = rememberInlineSave(editor)
    var editing by remember { mutableStateOf<RoutineDraft?>(null) }
    val own = e.workspace.routines.filter { it.bot == e.name }
    if (own.isEmpty()) Quiet("No Routines yet. Ask ${e.name} for one in a chat; it proposes it for you to apply.")
    for (r in own) {
        val live = model.routines.firstOrNull { it.workspace == e.workspace.name && it.name == r.name }
        RoutineCard(r, live, !editor.readOnly, { on ->
            save.run(listOf(SettingsOp.RoutineUpdate(e.workspace.name, r.spec.copy(enabled = on)))) { model.refreshRoutines() }
        }, { live?.let(model::runRoutine) }) { editing = RoutineDraft.of(r) }
    }
    save.Error()
    SecondaryButton("Add Routine", { editing = RoutineDraft(isNew = true, bot = e.name) }, Modifier.testTag("bot.routine.add"), enabled = !editor.readOnly)
    model.requestError?.let { Text(it, color = Theme.failure, fontSize = 13.sp) }
    val workers = editor.settings?.workers.orEmpty().map { it.name }
    editing?.let { RoutineDialog(editor, e.workspace.name, workers, e.workspace.bots.map { b -> b.name }, it) { editing = null } }
}

@Composable
private fun RoutineCard(
    r: SettingsRoutine,
    live: Routine?,
    canEdit: Boolean,
    toggle: (Boolean) -> Unit,
    run: () -> Unit,
    edit: () -> Unit,
) {
    Column(
        Modifier.fillMaxWidth().background(Palette.raised, CARD).padding(horizontal = 14.dp, vertical = 10.dp).testTag("bot.routine.${r.name}"),
        verticalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Column(Modifier.weight(1f)) {
                Text(r.name, fontSize = 15.sp)
                Text(live?.whenText ?: if (r.enabled) "Not running on the host" else "Off", fontSize = 13.sp, color = Palette.textTertiary)
            }
            Switch(r.enabled, toggle, Modifier.testTag("bot.routine.${r.name}.enabled"), enabled = canEdit)
        }
        Text(lastRun(live), fontSize = 13.sp, color = if (live?.last?.ok == false) Theme.failure else Palette.textSecondary)
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            SecondaryButton(
                "Run Now", run, Modifier.testTag("bot.routine.${r.name}.run"),
                enabled = live != null && live.enabled && live.runningSince == null,
            )
            SecondaryButton("Edit", edit, Modifier.testTag("bot.routine.${r.name}.edit"), enabled = canEdit)
        }
    }
}

private fun lastRun(r: Routine?): String {
    if (r?.runningSince != null) return "Running since ${time(r.runningSince ?: 0)}"
    val last = r?.last ?: return "Never run"
    val result = if (last.ok) "OK" + (last.published?.let { ", $it items" } ?: "") else "Failed" + (last.reason?.let { ": $it" } ?: "")
    return "Last run ${time(last.at)} · $result"
}

/** Files: the Soul, which the user writes, and the Memory, which the Bot writes, with its earlier versions. */
@Composable
fun ColumnScope.BotFiles(editor: SettingsEditor, e: BotEntry) {
    val scope = rememberCoroutineScope()
    var editingSoul by remember { mutableStateOf(false) }
    var restoring by remember { mutableStateOf<Int?>(null) }
    var error by remember { mutableStateOf<String?>(null) }
    Text("soul.md", style = MaterialTheme.typography.titleSmall)
    Text("Who the Bot is. You write it.", fontSize = 13.sp, color = Palette.textTertiary)
    FileBox(e.bot.soulText?.takeIf { it.isNotBlank() } ?: if (e.bot.soulText == null) "soul.md could not be read." else "No Soul yet.", "bot.soul.text")
    SecondaryButton("Edit Soul", { editingSoul = true }, Modifier.testTag("bot.soul.edit"), enabled = !editor.readOnly)
    MemoryText(e.bot.memoryText, "bot.memory")
    Text("The Bot's own notes, written after each run and chat.", fontSize = 13.sp, color = Palette.textTertiary)
    val versions = e.bot.memoryHistory.ifEmpty { (1..e.bot.memoryVersions).map { MemoryVersion(it, 0) } }
    if (versions.isNotEmpty()) Text("Earlier versions", style = MaterialTheme.typography.titleSmall)
    for (v in versions) {
        Row(Modifier.fillMaxWidth().testTag("bot.memory.version.${v.version}"), verticalAlignment = Alignment.CenterVertically) {
            Text(
                if (v.at > 0) "Version ${v.version} · replaced ${time(v.at)}" else "Version ${v.version}",
                Modifier.weight(1f), fontSize = 14.sp,
            )
            SecondaryButton("Restore", { restoring = v.version }, Modifier.testTag("bot.memory.restore.${v.version}"), enabled = !editor.readOnly)
        }
    }
    error?.let { Text(it, Modifier.testTag("bot.files.error"), color = Theme.failure, fontSize = 13.sp) }
    if (editingSoul) SoulDialog(editor, e) { editingSoul = false }
    restoring?.let { n ->
        ConfirmDialog(
            "Restore memory version $n?",
            "That memory comes back. The current one is kept as an earlier version.",
            "Restore",
            {
                restoring = null
                scope.launch { error = editor.save(listOf(SettingsOp.BotMemoryRestore(e.workspace.name, e.name, n))) }
            },
        ) { restoring = null }
    }
}

@Composable
private fun SoulDialog(editor: SettingsEditor, e: BotEntry, close: () -> Unit) {
    var d by remember { mutableStateOf(BotDraft.of(e.bot)) }
    FormDialog("${e.name}'s Soul", "bot.soul.form", d.problem == null, { editor.save(listOf(d.op(e.workspace.name))) }, close) {
        if (d.originalSoul == null) Text("soul.md could not be read. Typing here replaces it.", style = MaterialTheme.typography.bodySmall)
        Field(d.soul, { d = d.copy(soul = it) }, "Soul", "bot.soul", mono = true, lines = SOUL_LINES,
            help = "Who the Bot is: its purpose, its scope and how it writes. At most 12 KiB.")
    }
}

@Composable
private fun FileBox(text: String, tag: String) {
    SelectionContainer(Modifier.fillMaxWidth().background(Palette.surface, BOX).border(1.dp, Palette.border, BOX).padding(10.dp)) {
        Text(text, Modifier.testTag(tag), fontFamily = FontFamily.Monospace, fontSize = 12.5.sp)
    }
}

private fun time(unix: Long): String =
    DateTimeFormatter.ofLocalizedDateTime(FormatStyle.MEDIUM, FormatStyle.SHORT).format(Instant.ofEpochSecond(unix).atZone(ZoneId.systemDefault()))

private val CARD = RoundedCornerShape(16.dp)
private val BOX = RoundedCornerShape(8.dp)
private const val SOUL_LINES = 10
