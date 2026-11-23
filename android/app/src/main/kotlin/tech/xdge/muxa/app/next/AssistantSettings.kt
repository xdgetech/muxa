package tech.xdge.muxa.app.next

import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import tech.xdge.muxa.app.ui.KindPalette
import tech.xdge.muxa.v2.SettingsAssistant

/** The host's Capture assistant (ADR 0084). Hidden when the host is older and has none. */
@Composable
fun AssistantSection(editor: SettingsEditor) {
    val assistant = editor.settings?.assistant ?: return
    var editing by remember { mutableStateOf<AssistantDraft?>(null) }
    SettingsHeader("Capture assistant")
    SettingsCard {
        EntryRow("settings.assistant", !editor.readOnly, { editing = AssistantDraft.of(assistant) }, badge = null) {
            EntryTitle("Capture assistant") { if (!assistant.enabled) Chip("off", KindPalette.todo) }
            EntryDetail(assistant.worker?.let { "worker $it" } ?: "The default agent")
        }
    }
    SettingsNote("Each item you capture gets one short run. It suggests a kind, a Workspace, a due date, a Task or a Bot to ask.")
    val workers = editor.settings?.workers.orEmpty().map { it.name }
    editing?.let { AssistantDialog(editor, assistant, workers, it) { editing = null } }
}

@Composable
private fun AssistantDialog(editor: SettingsEditor, assistant: SettingsAssistant, workers: List<String>, initial: AssistantDraft, close: () -> Unit) {
    var d by remember { mutableStateOf(initial) }
    FormDialog("Capture Assistant", "settings.assistant.form", true, { editor.save(listOf(d.op())) }, close) {
        SwitchField("Enabled", "assistant.enabled", d.enabled) { d = d.copy(enabled = it) }
        NamePicker("Worker", "assistant.worker", workers, d.worker) { d = d.copy(worker = it) }
        if (d.originalSoul == null) {
            Text("soul.md could not be read. Typing here replaces it.", style = MaterialTheme.typography.bodySmall)
        }
        Field(d.soul, { d = d.copy(soul = it) }, "Soul", "assistant.soul", mono = true, lines = SOUL_LINES,
            help = "How it sorts what you capture. It has no Workspace and no connections.")
        MemoryText(assistant.memoryText, "assistant.memory")
    }
}

private const val SOUL_LINES = 6
