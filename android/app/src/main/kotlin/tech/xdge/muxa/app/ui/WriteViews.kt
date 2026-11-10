package tech.xdge.muxa.app.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import tech.xdge.muxa.app.AppModel
import tech.xdge.muxa.app.approveWrite
import tech.xdge.muxa.app.canWrite
import tech.xdge.muxa.app.openRunSession
import tech.xdge.muxa.app.openWriteItem
import tech.xdge.muxa.app.taskRecord
import tech.xdge.muxa.app.writesOf
import tech.xdge.muxa.protocol.AssistResult
import tech.xdge.muxa.protocol.AssistantRequest
import tech.xdge.muxa.protocol.InboxItem
import tech.xdge.muxa.protocol.ItemKind
import tech.xdge.muxa.protocol.ResultWrite
import tech.xdge.muxa.protocol.WriteProposal
import tech.xdge.muxa.session.WriteState
import tech.xdge.muxa.session.isOlder
import tech.xdge.muxa.session.itemOf
import tech.xdge.muxa.session.label
import tech.xdge.muxa.session.proposer
import tech.xdge.muxa.session.state
import tech.xdge.muxa.session.taskOf

// Write proposals in the app (ADR 0052 §7). The content is plain text in a fixed-width font: no
// Markdown and no active links, so the text the user approves is the text the user saw.

/** A `write` item's write: action, target, the whole content, who proposed it, and whether it is older. */
@Composable
internal fun WriteView(model: AppModel, item: InboxItem) {
    if (item.kind != ItemKind.WRITE) return
    val w = item.write ?: return
    Text("Proposed write", Modifier.testTag("item-write"), style = MaterialTheme.typography.titleSmall)
    Field("Action", w.action)
    Field("Target", w.target, mono = true)
    Column {
        Text("Content", style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
        SelectionContainer {
            Text(w.content, Modifier.fillMaxWidth().testTag("item-write-content"), fontFamily = FontFamily.Monospace)
        }
    }
    Field("From", w.proposer(item.workspace) + (w.step?.let { " (step $it)" } ?: ""))
    if (w.isOlder(model.state.requests)) {
        DetailNote("From an older result (v${w.version}). Its text is still as shown.", Modifier.testTag("item-write-older"))
    }
    val writer = model.writesOf(item.workspace)?.writer?.worker ?: "the writer"
    DetailNote("Approve starts one Task of $writer that writes this content, unchanged. muxa does not check what it writes.")
}

/** An approved write: "Task started", then the Task's own state, and its session. */
@Composable
internal fun WriteStarted(model: AppModel, item: InboxItem) {
    val task = item.task ?: return
    Box(Modifier.fillMaxSize().padding(32.dp), contentAlignment = Alignment.Center) {
        Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(8.dp)) {
            TaskLine(model, task, "item-write-task")
        }
    }
}

/** "Task started · Open" and Open Session while the Task has one. [tag] names the line and its button. */
@Composable
private fun TaskLine(model: AppModel, task: String, tag: String) {
    val record = model.taskRecord(task)
    val status = record?.let { " · ${statusTitle(it.status)}" }.orEmpty()
    Text("${WriteState.STARTED.label}$status", Modifier.testTag(tag), color = Theme.connected)
    val session = record?.let(model::sessionOf) ?: return
    TextButton({ model.openRunSession(session) }, Modifier.testTag("$tag-session")) { Text("Open Session") }
}

/** Approve's confirmation: the target once more. Confirm sends the hash of [write], the write shown. */
@Composable
internal fun ApproveWriteDialog(model: AppModel, item: InboxItem, write: WriteProposal) {
    val writer = model.writesOf(item.workspace)?.writer?.worker ?: "the writer"
    AlertDialog(
        onDismissRequest = { model.inboxPrompt = null },
        title = { Text("Approve this write?") },
        text = {
            Column(Modifier.fillMaxWidth().verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                Field("Action", write.action)
                Column(Modifier.testTag("approve-write-target")) { Field("Target", write.target, mono = true) }
                Text(
                    "$writer writes the content you read, unchanged, in one Task in ${item.workspace}.",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
        },
        confirmButton = { TextButton({ model.approveWrite(item, write) }, Modifier.testTag("approve-write-confirm")) { Text("Approve") } },
        dismissButton = { TextButton({ model.inboxPrompt = null }, Modifier.testTag("approve-write-cancel")) { Text("Cancel") } },
    )
}

/**
 * Under a Result in Work detail: its writes with their state. A waiting write opens its item, where
 * Approve is; an approved one names the writer's Task and opens its session.
 */
@Composable
internal fun ResultWrites(model: AppModel, request: AssistantRequest, result: AssistResult) {
    if (!model.canWrite || result.writes.isEmpty()) return
    val key = "${request.id}-${result.version}-${result.step ?: 1}"
    Column(Modifier.fillMaxWidth().testTag("result-writes-$key"), verticalArrangement = Arrangement.spacedBy(4.dp)) {
        DetailHeader("Writes")
        if (result.isOlder(request)) DetailNote("From an older result", Modifier.testTag("result-writes-older-$key"))
        result.writes.forEachIndexed { index, w -> WriteRow(model, w, "write-$key-$index") }
    }
}

@Composable
private fun WriteRow(model: AppModel, w: ResultWrite, tag: String) {
    val items = model.state.inboxItems
    Column(Modifier.fillMaxWidth().testTag(tag)) {
        Text(w.action)
        Text(w.target, fontFamily = FontFamily.Monospace, style = MaterialTheme.typography.bodySmall)
        val task = w.taskOf(items)
        if (task != null) {
            TaskLine(model, task, "$tag-task")
            return@Column
        }
        val state = w.state(items)
        DetailNote(state.label, Modifier.testTag("$tag-state"))
        val item = w.itemOf(items)
        if (state == WriteState.WAITING && item != null) {
            TextButton({ model.openWriteItem(item.id) }, Modifier.testTag("$tag-open")) { Text("Open Item") }
        }
    }
}

/** Where staff show: the workspace's writer, marked "writes", with its description, and the `writes` mode. */
@Composable
internal fun WriterNote(model: AppModel, workspace: String?, tag: String) {
    val d = model.writesOf(workspace) ?: return
    Column(Modifier.fillMaxWidth().testTag(tag)) {
        d.writer?.let { w ->
            Text("${w.worker} · writes", style = MaterialTheme.typography.labelMedium)
            DetailNote(w.description)
        }
        d.writes?.let { DetailNote(it.label, Modifier.testTag("$tag-mode")) }
    }
}
