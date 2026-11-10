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
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import tech.xdge.muxa.app.AppModel
import tech.xdge.muxa.app.canPlan
import tech.xdge.muxa.app.changeWork
import tech.xdge.muxa.app.listedRequests
import tech.xdge.muxa.app.loadWorkDetail
import tech.xdge.muxa.app.openTaskRow
import tech.xdge.muxa.app.startWorkTask
import tech.xdge.muxa.app.workItem
import tech.xdge.muxa.protocol.WorkAction
import tech.xdge.muxa.protocol.WorkItem
import tech.xdge.muxa.protocol.WorkState
import tech.xdge.muxa.session.WorkDetail as WorkRead
import tech.xdge.muxa.session.isClosed
import tech.xdge.muxa.session.isPinned
import tech.xdge.muxa.session.requestsOf
import tech.xdge.muxa.session.taskItem

/**
 * One Work item, read with `get-work` on open: Captures → Request → Result → Tasks → the decision
 * it waits on (ADR 0048 §10). A Task that waits on a decision is answered in its Inbox item, not here.
 */
@Composable
fun WorkDetail(model: AppModel, id: String) {
    val detail = model.state.workDetails[id]
    var loadError by remember(id) { mutableStateOf<String?>(null) }
    var loaded by remember(id) { mutableStateOf(false) }
    LaunchedEffect(id) {
        loadError = model.loadWorkDetail(id)
        loaded = true
    }
    // The list has the item's newest state; `get-work` has it also when the list does not.
    val item = model.workItem(id) ?: detail?.item
    if (item == null) {
        // Gone from the host, here or on another device.
        if (loaded) LaunchedEffect(id) { model.workDetail = null }
        return
    }
    val requests = requestsOf(item, model.listedRequests, detail?.requests.orEmpty())
    val busy = model.changingWork != null
    AlertDialog(
        onDismissRequest = { model.workDetail = null },
        title = { Text(item.title, Modifier.testTag("work-detail-title")) },
        text = {
            Column(
                Modifier.heightIn(max = 520.dp).verticalScroll(rememberScrollState()).testTag("work-detail"),
                verticalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                Text(item.subtitle, Modifier.testTag("work-detail-state"), color = MaterialTheme.colorScheme.onSurfaceVariant)
                TodayReason(model, item)
                Captures(detail, loadError)
                for (request in requests) RequestSection(model, item, request)
                NotesSection(model, requests)
                AskPanel(model, item, requests)
                LinkedTasks(model, item)
                if (item.state == WorkState.WAITING_DECISION) {
                    Text(
                        "A Task waits on your decision. Answer it under Decisions.",
                        Modifier.testTag("work-detail-waiting"),
                        style = MaterialTheme.typography.bodySmall,
                    )
                }
                Actions(model, item, busy)
            }
        },
        confirmButton = { TextButton({ model.workDetail = null }) { Text("Close") } },
    )
}

/** Each Capture's whole text, as the user wrote it, oldest link first. */
@Composable
private fun Captures(detail: WorkRead?, loadError: String?) {
    DetailHeader("Captures")
    when {
        detail != null && detail.captures.isEmpty() -> DetailNote("No Captures.")
        detail != null -> for (c in detail.captures) {
            Column(Modifier.testTag("work-capture-${c.id}")) {
                SelectionContainer { Text(c.text, style = MaterialTheme.typography.bodyMedium) }
                DetailNote(listOfNotNull(AgentSummary.age(c.createdAt), c.source.raw, c.workspace).joinToString(" · "))
            }
        }
        loadError != null -> Text(loadError, Modifier.testTag("work-captures-error"), color = Theme.failure)
        else -> DetailNote("Loading…")
    }
}

/** Each linked Task by its title; a tap opens it as a Tasks row does. */
@Composable
private fun LinkedTasks(model: AppModel, item: WorkItem) {
    if (item.tasks.isEmpty()) return
    DetailHeader("Tasks")
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

/** Complete, Later and Withdraw on an open item, and at `work:10` Pin and Due; Reopen on a closed one. Start a Task on both. */
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
            if (model.canPlan) {
                if (item.isPinned) act("Unpin", "work-unpin", WorkAction.UNPIN)() else act("Pin", "work-pin", WorkAction.PIN)()
                DueButton(model, item, busy)
            }
        }
        if (model.canCreateTasks) {
            TextButton({ model.startWorkTask(item) }, Modifier.testTag("work-start-task"), enabled = !busy) { Text("Start a Task") }
        }
    }
}

@Composable
internal fun DetailHeader(text: String) = Text(text, style = MaterialTheme.typography.labelLarge)

@Composable
internal fun DetailNote(text: String, modifier: Modifier = Modifier) =
    Text(text, modifier, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
