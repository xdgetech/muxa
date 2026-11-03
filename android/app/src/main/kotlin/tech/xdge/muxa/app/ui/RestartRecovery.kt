package tech.xdge.muxa.app.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.MaterialTheme
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
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.launch
import tech.xdge.muxa.app.AppModel
import tech.xdge.muxa.protocol.RestartRecoveryAction
import tech.xdge.muxa.protocol.TaskRecord
import tech.xdge.muxa.session.PaneSession
import tech.xdge.muxa.session.displayName
import tech.xdge.muxa.session.explanation
import tech.xdge.muxa.session.recoveryActions
import tech.xdge.muxa.session.title
import tech.xdge.muxa.session.wireName

private data class RecoveryChoice(val record: TaskRecord, val operation: String, val action: RestartRecoveryAction)

@Composable
fun RestartRecoverySection(model: AppModel, record: TaskRecord) {
    val operation = record.restartOperation ?: return
    var choice by remember(record.id) { mutableStateOf<RecoveryChoice?>(null) }
    var saving by remember(record.id) { mutableStateOf(false) }
    var error by remember(record.id) { mutableStateOf<String?>(null) }
    val scope = rememberCoroutineScope()
    Column(Modifier.testTag("restart-recovery"), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Text("Restart needs recovery", style = MaterialTheme.typography.titleSmall)
        Text("Requested / unconfirmed: ${operation.assignment?.displayName ?: "Assignment unrecorded"}")
        Text("The interrupted operation must be resolved before restarting again.", style = MaterialTheme.typography.bodySmall)
        if (model.state.canRecoverRestarts) {
            for (action in operation.recoveryActions) {
                TextButton({ choice = RecoveryChoice(record, operation.id, action) }, enabled = !saving,
                    modifier = Modifier.testTag("recover-${action.wireName}")) { Text(action.title) }
            }
        }
        if (operation.recoveryActions.isEmpty() || !model.state.canRecoverRestarts) {
            Text("Open Terminal or update the host to inspect this operation.", style = MaterialTheme.typography.bodySmall)
        }
        error?.let { Text(it, color = MaterialTheme.colorScheme.error) }
    }
    choice?.let { selected ->
        AlertDialog(
            onDismissRequest = { choice = null },
            title = { Text("Resolve the interrupted restart?") },
            text = { Text(selected.action.explanation) },
            confirmButton = {
                TextButton({
                    choice = null
                    saving = true
                    scope.launch {
                        try { model.session.recoverRestart(selected.record, selected.operation, selected.action) }
                        catch (e: PaneSession.Failure) { error = PaneSession.describe(e) }
                        finally { saving = false }
                    }
                }) { Text(selected.action.title) }
            },
            dismissButton = { TextButton({ choice = null }) { Text("Cancel") } },
        )
    }
}
