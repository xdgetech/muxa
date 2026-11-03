package tech.xdge.muxa.app.ui

import android.content.ClipData
import android.content.ClipboardManager
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.setValue
import androidx.compose.runtime.remember
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import tech.xdge.muxa.app.AppModel
import tech.xdge.muxa.protocol.TaskRecord
import tech.xdge.muxa.protocol.WorkerSelection
import tech.xdge.muxa.session.allows
import tech.xdge.muxa.session.workerProblem
import tech.xdge.muxa.session.displayName
import tech.xdge.muxa.session.InboxItem
import tech.xdge.muxa.session.inboxItem
import tech.xdge.muxa.session.restartNeedsForce
import tech.xdge.muxa.session.runRows

/**
 * Asks before a Task's agent restarts (ADR 0018). A running agent gets only "Stop and
 * restart", so stopping its work is always an explicit choice. Labels match the iOS app.
 */
@Composable
fun RestartAgentDialog(model: AppModel) {
    val id = model.restartingTask ?: return
    val record = model.state.tasks?.record(id) ?: return
    val force = inboxItem(record, model.state.sessions).restartNeedsForce
    var choice by remember(id) { mutableStateOf<WorkerSelection>(WorkerSelection.Keep) }
    val workerIssue = workerProblem((choice as? WorkerSelection.Named)?.name, record.workspace, model.state.tasks?.workers.orEmpty())
    AlertDialog(
        onDismissRequest = { model.restartingTask = null },
        title = { Text("Restart the agent?") },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(RESTART_MESSAGE)
                if (model.state.canUseWorkers) {
                    WorkerRestartPicker(
                        model.state.tasks?.workers.orEmpty().filter { it.allows(record.workspace) },
                        record.worker, choice,
                    ) { choice = it }
                    workerIssue?.let { Text(it, color = MaterialTheme.colorScheme.error) }
                }
            }
        },
        confirmButton = {
            if (force) {
                TextButton({ model.restartAgent(record, force = true, worker = choice) }, Modifier.testTag("restart-force"), enabled = workerIssue == null) {
                    Text("Stop and restart", color = MaterialTheme.colorScheme.error)
                }
            } else {
                TextButton({ model.restartAgent(record, force = false, worker = choice) }, Modifier.testTag("restart-confirm"), enabled = workerIssue == null) { Text("Restart") }
            }
        },
        dismissButton = { TextButton({ model.restartingTask = null }) { Text("Cancel") } },
    )
}

@Composable
fun RestartErrorDialog(model: AppModel) {
    val error = model.restartError ?: return
    AlertDialog(
        onDismissRequest = { model.restartError = null },
        title = { Text("Can't Restart the Agent") },
        text = { Text(error, Modifier.testTag("restart-error-message")) },
        confirmButton = { TextButton({ model.restartError = null }) { Text("OK") } },
    )
}

/** The agents that worked on the Task, newest first, from its record's runs. */
@Composable
fun RunsSection(record: TaskRecord, item: InboxItem) {
    if (record.runs.isEmpty()) return
    val rows = runRows(record, agentReported = item.agent != null, now = System.currentTimeMillis() / 1000)
    Column(Modifier.testTag("task-runs"), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Text("Runs", style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
        for (row in rows) {
            Column(Modifier.testTag("run-${row.run.id}")) {
                Text(row.title, fontWeight = FontWeight.SemiBold)
                Text(row.detail, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                Text(row.run.assignment?.displayName ?: "Worker unrecorded", style = MaterialTheme.typography.bodySmall)
                row.run.agentSession?.let { AgentSessionId(it) }
            }
        }
    }
}

@Composable
private fun AgentSessionId(id: String) {
    val context = LocalContext.current
    Row(verticalAlignment = Alignment.CenterVertically) {
        SelectionContainer(Modifier.weight(1f, fill = false)) {
            Text(id, fontFamily = FontFamily.Monospace, style = MaterialTheme.typography.bodySmall)
        }
        IconButton({
            context.getSystemService(ClipboardManager::class.java)?.setPrimaryClip(ClipData.newPlainText("agent session id", id))
        }, Modifier.padding(start = 4.dp).size(32.dp)) {
            Icon(Icons.copy, "Copy session id", Modifier.size(16.dp))
        }
    }
}

private const val RESTART_MESSAGE =
    "The current agent process will be stopped. A new agent starts in the same pane and reads TASK.md and HANDOFF.md."
