package tech.xdge.muxa.app.next

import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import tech.xdge.muxa.v2.SettingsOp
import tech.xdge.muxa.v2.WorkerSpec

/** The host's workers: an agent with its args and instructions that Routines and Task Loops can name. */
@Composable
fun WorkersSection(editor: SettingsEditor, workspaces: List<String>) {
    var editing by remember { mutableStateOf<WorkerDraft?>(null) }
    val workers = editor.settings?.workers.orEmpty()
    SettingsHeader("Workers")
    SettingsCard {
        if (workers.isEmpty()) SettingsNote("No workers.")
        for (w in workers) {
            EntryRow("settings.worker.${w.name}", !editor.readOnly, { editing = WorkerDraft.of(w) }, badge = null) {
                EntryTitle(w.name)
                EntryDetail(workerLine(w))
            }
            CardDivider()
        }
        ActionRow("+ Add worker", "settings.worker.add", !editor.readOnly) { editing = WorkerDraft(isNew = true) }
    }
    editing?.let { WorkerDialog(editor, workspaces, it) { editing = null } }
}

private fun workerLine(w: WorkerSpec): String = listOfNotNull(
    w.agent,
    w.description,
    w.workspaces?.let { "only " + it.joinToString(", ") },
    w.loop?.let { "loop: ${it.check}" },
).joinToString(" · ")

@Composable
private fun WorkerDialog(editor: SettingsEditor, workspaces: List<String>, initial: WorkerDraft, close: () -> Unit) {
    var d by remember { mutableStateOf(initial) }
    FormDialog(
        if (d.isNew) "New Worker" else d.name, "settings.worker.form", d.isValid, { editor.save(listOf(d.op())) }, close,
        remove = if (d.isNew) null else ({ editor.save(listOf(SettingsOp.WorkerRemove(d.name))) }),
        removeTitle = "Remove Worker",
    ) {
        Field(d.name, { d = d.copy(name = it) }, "Name", "worker.name", enabled = d.isNew, placeholder = "reviewer")
        Field(d.description, { d = d.copy(description = it) }, "Description", "worker.description")
        Text("Agent", style = MaterialTheme.typography.titleSmall)
        Choice(listOf(WorkerDraft.CLAUDE_CODE, WorkerDraft.CODEX), listOf("Claude Code", "Codex"), d.agent, "worker.agent") {
            d = d.copy(agent = it)
        }
        Field(d.args, { d = d.copy(args = it) }, "Args", "worker.args", mono = true, lines = 2, help = "One per line.")
        Field(d.instructions, { d = d.copy(instructions = it) }, "Instruction file", "worker.instructions", mono = true,
            help = "A file on the host. Optional.")
        NameChips("Only in these Workspaces", workspaces, d.workspaces, "worker.workspace") { d = d.copy(workspaces = it) }
        Text("None picked: every Workspace.", style = MaterialTheme.typography.bodySmall)
        Field(d.check, { d = d.copy(check = it) }, "Default loop check", "worker.check", mono = true, placeholder = "make test",
            help = "The check its Task Loops run. Optional.")
    }
}
