package tech.xdge.muxa.app.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ExposedDropdownMenuBox
import androidx.compose.material3.ExposedDropdownMenuDefaults
import androidx.compose.material3.FilterChip
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.MenuAnchorType
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.launch
import tech.xdge.muxa.app.AppModel
import tech.xdge.muxa.app.askFromCapture
import tech.xdge.muxa.app.canTalkToAssistant
import tech.xdge.muxa.app.captureToTask
import tech.xdge.muxa.app.pickableWorkspaces
import tech.xdge.muxa.app.saveCapture
import tech.xdge.muxa.app.talkToAssistant
import tech.xdge.muxa.protocol.AssistScope
import tech.xdge.muxa.session.ASSIST_SCOPES
import tech.xdge.muxa.session.CaptureDraft
import tech.xdge.muxa.session.PaneSession
import tech.xdge.muxa.session.label

/**
 * `+` on a host with `work:9` (ADR 0048 §10): write it down now, sort it later. Save keeps the
 * text on the host and starts nothing. Ask Assistant also makes a Work item and asks the
 * workspace's assistant, so it needs a workspace. "Start a Task instead…" opens New Task with the text.
 */
@Composable
fun CaptureSheet(model: AppModel, initial: CaptureDraft) {
    var draft by remember { mutableStateOf(initial) }
    var saving by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }
    val scope = rememberCoroutineScope()
    fun edit(next: CaptureDraft) {
        draft = next
        // The sheet outlives a fold or the Reader: the model keeps the text.
        model.captureDraft = next
        error = null
    }
    fun send(ask: Boolean) {
        if ((if (ask) draft.askProblem else draft.problem) != null || saving) return
        saving = true
        scope.launch {
            try {
                if (ask) model.askFromCapture(draft) else model.saveCapture(draft)
            } catch (e: PaneSession.Failure) {
                error = PaneSession.describe(e)
            } finally {
                saving = false
            }
        }
    }
    AlertDialog(
        onDismissRequest = { if (!saving) model.captureDraft = null },
        title = { Text("Capture") },
        text = {
            Column(Modifier.heightIn(max = 520.dp).verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                OutlinedTextField(
                    draft.text, { edit(draft.copy(text = it)) }, Modifier.fillMaxWidth().testTag("capture-text"),
                    placeholder = { Text("An idea, a to-do, a question…") }, minLines = 4,
                    keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.Sentences),
                )
                // A member belongs to one workspace's staff: another workspace starts with its lead again.
                CaptureWorkspace(model, draft.workspace) { edit(draft.copy(workspace = it, worker = null)) }
                ScopePicker(draft.scope, "capture-scope") { edit(draft.copy(scope = it)) }
                StaffPicker(model, draft.workspace, draft.worker, "capture-staff") { edit(draft.copy(worker = it)) }
                WriterNote(model, draft.workspace, "capture-writer")
                MemoryNote(model, draft.workspace, "capture-memory")
                Text(
                    "Save keeps it in the Inbox under Unsorted. Nothing starts. " +
                        "Ask Assistant makes a Work item and asks the workspace's assistant.",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                TextButton(
                    { model.captureToTask(draft) }, Modifier.testTag("capture-start-task"),
                    enabled = !saving && draft.text.isNotBlank() && model.canCreateTasks,
                ) { Text("Start a Task instead…") }
                // Only on an empty sheet: the chat would leave a typed thought unsaved (ADR 0054).
                if (model.canTalkToAssistant && draft.text.isBlank()) {
                    TextButton(
                        { model.talkToAssistant() }, Modifier.testTag("capture-talk"), enabled = !saving,
                    ) { Text("Talk to Assistant") }
                }
                error?.let { Text(it, color = Theme.failure, modifier = Modifier.testTag("capture-error")) }
            }
        },
        confirmButton = {
            if (saving) {
                CircularProgressIndicator()
            } else {
                Row {
                    TextButton({ send(ask = true) }, enabled = draft.askProblem == null, modifier = Modifier.testTag("capture-ask")) {
                        Text("Ask Assistant")
                    }
                    TextButton({ send(ask = false) }, enabled = draft.problem == null, modifier = Modifier.testTag("capture-save")) { Text("Save") }
                }
            }
        },
        dismissButton = { TextButton({ model.captureDraft = null }, enabled = !saving) { Text("Cancel") } },
    )
}

/** Research, Compare or Draft: what the assistant is asked to do. */
@OptIn(ExperimentalLayoutApi::class)
@Composable
internal fun ScopePicker(scope: AssistScope, tag: String, pick: (AssistScope) -> Unit) {
    FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        for (s in ASSIST_SCOPES) {
            FilterChip(s == scope, { pick(s) }, { Text(s.label) }, Modifier.testTag("$tag-${s.raw}"))
        }
    }
}

/** Optional for Save, needed by Ask Assistant: "No workspace" until the user picks one. */
@Composable
private fun CaptureWorkspace(model: AppModel, workspace: String?, pick: (String?) -> Unit) =
    WorkspacePicker(model, workspace, "Workspace (optional for Save)", "capture-workspace", pick)

/** A workspace from the host's list, without the overall assistant's, or "No workspace"; [tag] names the field and, with a suffix, each choice. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
internal fun WorkspacePicker(model: AppModel, workspace: String?, label: String, tag: String, pick: (String?) -> Unit) {
    val workspaces = model.pickableWorkspaces
    if (workspaces.isEmpty()) return
    var picking by remember { mutableStateOf(false) }
    ExposedDropdownMenuBox(picking, { picking = it }) {
        OutlinedTextField(
            workspace ?: NO_WORKSPACE, {},
            Modifier.fillMaxWidth().menuAnchor(MenuAnchorType.PrimaryNotEditable).testTag(tag),
            readOnly = true, singleLine = true, label = { Text(label) },
            trailingIcon = { ExposedDropdownMenuDefaults.TrailingIcon(picking) },
        )
        ExposedDropdownMenu(picking, { picking = false }) {
            DropdownMenuItem({ Text(NO_WORKSPACE) }, {
                pick(null)
                picking = false
            }, modifier = Modifier.testTag("$tag-none"))
            for (w in workspaces) {
                DropdownMenuItem({ Text(w.name) }, {
                    pick(w.name)
                    picking = false
                }, modifier = Modifier.testTag("$tag-${w.name}"))
            }
        }
    }
}

private const val NO_WORKSPACE = "No workspace"

/** After a Save that could not reach the host. */
@Composable
fun CaptureNotice(model: AppModel) {
    val notice = model.captureNotice ?: return
    AlertDialog(
        onDismissRequest = { model.captureNotice = null },
        title = { Text("Saved on This Device") },
        text = { Text(notice, Modifier.testTag("capture-notice")) },
        confirmButton = { TextButton({ model.captureNotice = null }) { Text("OK") } },
    )
}

/** After Ask Assistant saved the Capture but the assistant did not start: the host's reason. */
@Composable
fun AssistNotice(model: AppModel) {
    val notice = model.assistNotice ?: return
    AlertDialog(
        onDismissRequest = { model.assistNotice = null },
        title = { Text("Assistant Not Started") },
        text = { Text(notice, Modifier.testTag("assist-notice")) },
        confirmButton = { TextButton({ model.assistNotice = null }) { Text("OK") } },
    )
}
