package tech.xdge.muxa.app.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
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
import tech.xdge.muxa.app.captureToTask
import tech.xdge.muxa.app.saveCapture
import tech.xdge.muxa.session.CaptureDraft
import tech.xdge.muxa.session.PaneSession

/**
 * `+` on a host with `work:8` (ADR 0046 §10): write it down now, sort it later. Save keeps the
 * text on the host and starts nothing. "Start a Task instead…" opens New Task with the text.
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
    fun save() {
        if (draft.problem != null || saving) return
        saving = true
        scope.launch {
            try {
                model.saveCapture(draft)
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
                CaptureWorkspace(model, draft.workspace) { edit(draft.copy(workspace = it)) }
                Text(
                    "Save keeps it in the Inbox under Unsorted. Nothing starts.",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                TextButton(
                    { model.captureToTask(draft) }, Modifier.testTag("capture-start-task"),
                    enabled = !saving && draft.text.isNotBlank() && model.canCreateTasks,
                ) { Text("Start a Task instead…") }
                error?.let { Text(it, color = Theme.failure, modifier = Modifier.testTag("capture-error")) }
            }
        },
        confirmButton = {
            if (saving) {
                CircularProgressIndicator()
            } else {
                TextButton(::save, enabled = draft.problem == null, modifier = Modifier.testTag("capture-save")) { Text("Save") }
            }
        },
        dismissButton = { TextButton({ model.captureDraft = null }, enabled = !saving) { Text("Cancel") } },
    )
}

/** Optional: "No workspace" until the user picks one. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun CaptureWorkspace(model: AppModel, workspace: String?, pick: (String?) -> Unit) {
    val workspaces = model.state.tasks?.workspaces.orEmpty()
    if (workspaces.isEmpty()) return
    var picking by remember { mutableStateOf(false) }
    ExposedDropdownMenuBox(picking, { picking = it }) {
        OutlinedTextField(
            workspace ?: NO_WORKSPACE, {},
            Modifier.fillMaxWidth().menuAnchor(MenuAnchorType.PrimaryNotEditable).testTag("capture-workspace"),
            readOnly = true, singleLine = true, label = { Text("Workspace (optional)") },
            trailingIcon = { ExposedDropdownMenuDefaults.TrailingIcon(picking) },
        )
        ExposedDropdownMenu(picking, { picking = false }) {
            DropdownMenuItem({ Text(NO_WORKSPACE) }, {
                pick(null)
                picking = false
            }, modifier = Modifier.testTag("capture-workspace-none"))
            for (w in workspaces) {
                DropdownMenuItem({ Text(w.name) }, {
                    pick(w.name)
                    picking = false
                }, modifier = Modifier.testTag("capture-workspace-${w.name}"))
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
