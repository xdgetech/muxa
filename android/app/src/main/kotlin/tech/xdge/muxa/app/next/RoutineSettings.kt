package tech.xdge.muxa.app.next

import androidx.compose.foundation.layout.Box
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import tech.xdge.muxa.app.ui.KindPalette
import tech.xdge.muxa.app.ui.SecondaryButton
import tech.xdge.muxa.v2.SettingsOp
import tech.xdge.muxa.v2.SettingsRoutine
import tech.xdge.muxa.v2.SettingsWorkspace

/** A Workspace's Routines, as its `.muxa/routines.toml` has them. They show while the gate is off too. */
@Composable
fun RoutineSettings(editor: SettingsEditor, ws: SettingsWorkspace) {
    var editing by remember { mutableStateOf<RoutineDraft?>(null) }
    if (!ws.routinesEnabled) SettingsNote("Routines are not allowed in ${ws.name}: none of these runs. Allow them under General.")
    ws.routinesProblem?.let { SettingsWarning("routines.toml cannot be used as it is: $it", "settings.routines.problem") }
    SettingsHeader("Routines")
    SettingsCard {
        if (ws.routines.isEmpty()) SettingsNote("No Routines.")
        for (r in ws.routines) {
            EntryRow("settings.routine.${ws.name}.${r.name}", !editor.readOnly, { editing = RoutineDraft.of(r) }, badge = null) {
                EntryTitle(r.name) { if (!r.enabled) Chip("off", KindPalette.todo) }
                EntryDetail(scheduleText(r))
                EntryDetail(".muxa/${r.instructions}", mono = true)
            }
            CardDivider()
        }
        ActionRow("+ Add Routine", "settings.routine.add", !editor.readOnly) { editing = RoutineDraft(isNew = true) }
    }
    val workers = editor.settings?.workers.orEmpty().map { it.name }
    editing?.let { RoutineDialog(editor, ws.name, workers, it) { editing = null } }
}

private fun scheduleText(r: SettingsRoutine): String {
    val whenText = r.schedule?.let { "cron $it" } ?: r.at?.let { "at $it" } ?: "only when run by hand"
    return listOfNotNull(whenText, r.timezone, r.worker?.let { "worker $it" }, r.items?.let { "$it items" }).joinToString(" · ")
}

@Composable
private fun RoutineDialog(editor: SettingsEditor, workspace: String, workers: List<String>, initial: RoutineDraft, close: () -> Unit) {
    var d by remember { mutableStateOf(initial) }
    FormDialog(
        if (d.isNew) "New Routine" else d.name, "settings.routine.form", d.problem == null, { editor.save(listOf(d.op(workspace))) }, close,
        remove = if (d.isNew) null else ({ editor.save(listOf(SettingsOp.RoutineRemove(workspace, d.name))) }),
        removeTitle = "Remove Routine",
    ) {
        Field(d.name, { d = d.copy(name = it) }, "Name", "routine.name", enabled = d.isNew, placeholder = "daily-digest")
        Choice(listOf(false, true), listOf("At a time", "Cron"), d.cron, "routine.kind") { d = d.copy(cron = it) }
        if (d.cron) {
            Field(d.schedule, { d = d.copy(schedule = it) }, "Schedule", "routine.schedule", mono = true, placeholder = "0 9 * * 1-5")
        } else {
            Field(d.at, { d = d.copy(at = it) }, "At", "routine.at", mono = true, placeholder = "09:00", help = "Empty: only when run by hand.")
        }
        Field(d.timezone, { d = d.copy(timezone = it) }, "Time zone", "routine.timezone", placeholder = "The host's")
        WorkerPicker(workers, d.worker) { d = d.copy(worker = it) }
        Field(d.instructions, { d = d.copy(instructions = it) }, "Instruction file", "routine.file", mono = true,
            placeholder = "routines/${d.name.trim().ifEmpty { "name" }}.md", help = "In .muxa/.")
        if (!d.isNew && d.originalText == null) {
            Text("The instruction file could not be read. Typing here replaces it.", style = MaterialTheme.typography.bodySmall)
        }
        Field(d.text, { d = d.copy(text = it) }, "Instructions", "routine.instructions", mono = true, lines = INSTRUCTION_LINES)
        Field(d.items, { d = d.copy(items = it) }, "Items", "routine.items", number = true, help = "At most this many Inbox items per run. Empty: the default.")
        SwitchField("Enabled", "routine.enabled", d.enabled) { d = d.copy(enabled = it) }
    }
}

@Composable
private fun WorkerPicker(workers: List<String>, picked: String?, pick: (String?) -> Unit) {
    var open by remember { mutableStateOf(false) }
    Box {
        SecondaryButton("Worker: ${picked ?: "none"}", { open = true }, Modifier.testTag("routine.worker"))
        DropdownMenu(open, { open = false }) {
            for (w in listOf<String?>(null) + workers) {
                DropdownMenuItem({ Text(w ?: "none") }, {
                    open = false
                    pick(w)
                }, Modifier.testTag("routine.worker.${w ?: "none"}"))
            }
        }
    }
}

private const val INSTRUCTION_LINES = 8
