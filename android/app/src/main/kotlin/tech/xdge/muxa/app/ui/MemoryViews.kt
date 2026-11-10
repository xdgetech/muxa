package tech.xdge.muxa.app.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import tech.xdge.muxa.app.AppModel
import tech.xdge.muxa.app.approveMemory
import tech.xdge.muxa.app.memoryItemOf
import tech.xdge.muxa.app.memoryOf
import tech.xdge.muxa.app.openMemoryItem
import tech.xdge.muxa.protocol.InboxItem
import tech.xdge.muxa.protocol.ItemKind
import tech.xdge.muxa.protocol.MemoryProposal
import tech.xdge.muxa.protocol.TaskRecord
import tech.xdge.muxa.protocol.TaskStatus
import tech.xdge.muxa.session.MemoryState
import tech.xdge.muxa.session.label
import tech.xdge.muxa.session.line
import tech.xdge.muxa.session.memoryState

// Workspace memory in the app (ADR 0053 §7). The text is plain text in a fixed-width font: no
// Markdown and no active links, so the text the user approves is the text the user saw.

/** A `memory` item's proposal: the whole text, and where Approve adds it. */
@Composable
internal fun MemoryView(model: AppModel, item: InboxItem) {
    if (item.kind != ItemKind.MEMORY) return
    val m = item.memory ?: return
    Text("Proposed for the workspace memory", Modifier.testTag("item-memory"), style = MaterialTheme.typography.titleSmall)
    Column {
        Text("Text", style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
        SelectionContainer {
            Text(m.text, Modifier.fillMaxWidth().testTag("item-memory-text"), fontFamily = FontFamily.Monospace)
        }
    }
    DetailNote("Approve adds this text, unchanged, to .muxa/WORKSPACE.md in ${item.workspace}. Every later run there reads it.")
}

/** Approve's confirmation: the workspace once more. Confirm sends the hash of [memory], the text shown. */
@Composable
internal fun ApproveMemoryDialog(model: AppModel, item: InboxItem, memory: MemoryProposal) {
    AlertDialog(
        onDismissRequest = { model.inboxPrompt = null },
        title = { Text("Add to the workspace memory?") },
        text = {
            Column(Modifier.fillMaxWidth().verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                Column(Modifier.testTag("approve-memory-workspace")) { Field("Workspace", item.workspace) }
                Text(
                    "The text you read goes under \"${memory.title}\" in .muxa/WORKSPACE.md, unchanged.",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
        },
        confirmButton = { TextButton({ model.approveMemory(item, memory) }, Modifier.testTag("approve-memory-confirm")) { Text("Add") } },
        dismissButton = { TextButton({ model.inboxPrompt = null }, Modifier.testTag("approve-memory-cancel")) { Text("Cancel") } },
    )
}

/** After a Task's end: its proposal to the workspace memory, with the item's state. An open one opens its item. */
@Composable
internal fun TaskMemoryLine(model: AppModel, record: TaskRecord) {
    if (record.status != TaskStatus.DONE && record.status != TaskStatus.DROPPED) return
    val item = model.memoryItemOf(record.id) ?: return
    val state = item.memoryState
    Column(Modifier.fillMaxWidth()) {
        Text(state.line, Modifier.testTag("task-memory"), color = if (state == MemoryState.ADDED) Theme.connected else MaterialTheme.colorScheme.onSurface)
        if (state == MemoryState.WAITING || state == MemoryState.MOVED_DOWN) {
            TextButton({ model.openMemoryItem(item.id) }, Modifier.testTag("task-memory-open")) { Text("Open Item") }
        }
    }
}

/** Where staff show: the workspace's `memory` mode, and when WORKSPACE.md last changed. */
@Composable
internal fun MemoryNote(model: AppModel, workspace: String?, tag: String) {
    val w = model.memoryOf(workspace) ?: return
    val mode = w.memory ?: return
    val changed = w.memoryAt?.let { "WORKSPACE.md changed ${AgentSummary.age(it)} ago." } ?: "No WORKSPACE.md yet."
    Column(Modifier.fillMaxWidth().testTag(tag)) {
        Text("Workspace memory", style = MaterialTheme.typography.labelMedium)
        DetailNote(mode.label, Modifier.testTag("$tag-mode"))
        DetailNote(changed, Modifier.testTag("$tag-at"))
    }
}
