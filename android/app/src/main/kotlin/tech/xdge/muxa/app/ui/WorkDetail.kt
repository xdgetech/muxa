package tech.xdge.muxa.app.ui

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import tech.xdge.muxa.app.AppModel
import tech.xdge.muxa.app.changeWork
import tech.xdge.muxa.app.openTaskRow
import tech.xdge.muxa.app.startWorkTask
import tech.xdge.muxa.app.workItem
import tech.xdge.muxa.protocol.WorkAction
import tech.xdge.muxa.protocol.WorkItem
import tech.xdge.muxa.protocol.WorkState
import tech.xdge.muxa.session.isClosed
import tech.xdge.muxa.session.taskItem

/**
 * One Work item: its state, workspace, the Captures and Tasks linked to it, and what the user
 * can do. A Task that waits on a decision is answered in its Inbox item, not here (ADR 0046 §10).
 */
@Composable
fun WorkDetail(model: AppModel, id: String) {
    val item = model.workItem(id)
    if (item == null) {
        // Gone from the host's list, here or on another device.
        LaunchedEffect(id) { model.workDetail = null }
        return
    }
    val busy = model.changingWork != null
    AlertDialog(
        onDismissRequest = { model.workDetail = null },
        title = { Text(item.title, Modifier.testTag("work-detail-title")) },
        text = {
            Column(
                Modifier.heightIn(max = 480.dp).verticalScroll(rememberScrollState()).testTag("work-detail"),
                verticalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                Text(item.subtitle, Modifier.testTag("work-detail-state"), color = MaterialTheme.colorScheme.onSurfaceVariant)
                if (item.state == WorkState.WAITING_DECISION) {
                    Text("A Task waits on your decision. Answer it under Decisions.", style = MaterialTheme.typography.bodySmall)
                }
                Text("Captures: ${item.captures.size}", style = MaterialTheme.typography.bodySmall)
                LinkedTasks(model, item)
                Actions(model, item, busy)
            }
        },
        confirmButton = { TextButton({ model.workDetail = null }) { Text("Close") } },
    )
}

/** Each linked Task by its title; a tap opens it as a Tasks row does. */
@Composable
private fun LinkedTasks(model: AppModel, item: WorkItem) {
    if (item.tasks.isEmpty()) return
    Text("Tasks", style = MaterialTheme.typography.labelLarge)
    for (task in item.tasks) {
        val record = model.state.tasks?.record(task)
        Text(
            record?.title ?: task,
            Modifier.fillMaxWidth()
                .clickable(enabled = record != null) {
                    model.workDetail = null
                    record?.let { model.openTaskRow(taskItem(it, model.state.sessions)) }
                }
                .padding(vertical = 4.dp)
                .testTag("work-task-$task"),
            color = MaterialTheme.colorScheme.primary,
        )
    }
}

/** Complete, Later and Withdraw on an open item; Reopen on a closed one. Start a Task on both. */
@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun Actions(model: AppModel, item: WorkItem, busy: Boolean) {
    FlowRow(horizontalArrangement = Arrangement.spacedBy(4.dp)) {
        fun act(label: String, tag: String, action: WorkAction): @Composable () -> Unit = {
            TextButton({ model.changeWork(item, action) }, Modifier.testTag(tag), enabled = !busy) { Text(label) }
        }
        if (item.isClosed) {
            act("Reopen", "work-reopen", WorkAction.REOPEN)()
        } else {
            act("Complete", "work-complete", WorkAction.COMPLETE)()
            act("Later", "work-later", WorkAction.LATER)()
            act("Withdraw", "work-withdraw", WorkAction.WITHDRAW)()
        }
        if (model.canCreateTasks) {
            TextButton({ model.startWorkTask(item) }, Modifier.testTag("work-start-task"), enabled = !busy) { Text("Start a Task") }
        }
    }
}
