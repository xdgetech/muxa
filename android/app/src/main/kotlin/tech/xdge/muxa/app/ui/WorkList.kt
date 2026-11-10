package tech.xdge.muxa.app.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.SwipeToDismissBox
import androidx.compose.material3.SwipeToDismissBoxValue
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.rememberSwipeToDismissBoxState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.launch
import tech.xdge.muxa.app.AppModel
import tech.xdge.muxa.app.assistNote
import tech.xdge.muxa.app.changeWork
import tech.xdge.muxa.app.listedWork
import tech.xdge.muxa.app.sendNow
import tech.xdge.muxa.app.sort
import tech.xdge.muxa.app.unsentHere
import tech.xdge.muxa.app.unsortedCaptures
import tech.xdge.muxa.protocol.Capture
import tech.xdge.muxa.protocol.SortAction
import tech.xdge.muxa.protocol.WorkAction
import tech.xdge.muxa.protocol.WorkItem
import tech.xdge.muxa.session.RESULT_READY
import tech.xdge.muxa.session.UnsentCapture
import tech.xdge.muxa.session.firstLine
import tech.xdge.muxa.session.isLater
import tech.xdge.muxa.session.isPinned
import tech.xdge.muxa.session.label
import tech.xdge.muxa.session.stateLabel

// The Inbox's Work and Unsorted sections (ADR 0048 §10). The badge still counts decisions only.

/** The Inbox has Work items, Captures or unsent captures to show. */
val AppModel.hasWorkRows: Boolean get() = listedWork.isNotEmpty() || unsortedCaptures.isNotEmpty() || unsentHere.isNotEmpty()

/** Work, then Unsorted, each only when it has rows. Unsent captures lead Unsorted. */
fun LazyListScope.workSections(model: AppModel) {
    val work = model.listedWork
    if (work.isNotEmpty()) {
        item(key = "h-work") { InboxSectionHeader("Work", "work") }
        items(work, key = { it.id }) { w ->
            WorkRowView(model, w)
            HorizontalDivider()
        }
    }
    val unsent = model.unsentHere
    val captures = model.unsortedCaptures
    if (unsent.isEmpty() && captures.isEmpty()) return
    item(key = "h-unsorted") { InboxSectionHeader("Unsorted", "unsorted") }
    items(unsent, key = { "u-${it.clientId}" }) { c ->
        UnsentRow(model, c)
        HorizontalDivider()
    }
    items(captures, key = { it.id }) { c ->
        CaptureRow(model, c, work)
        HorizontalDivider()
    }
}

@Composable
internal fun InboxSectionHeader(text: String, tag: String) {
    Box(Modifier.testTag("inbox-section-$tag")) { SectionHeader(text) }
}

/** A swipe right completes the item, a swipe left is Later: it moves down, never hides (ADR 0040). */
@Composable
private fun WorkRowView(model: AppModel, item: WorkItem) {
    val swipe = rememberSwipeToDismissBoxState()
    val scope = rememberCoroutineScope()
    SwipeToDismissBox(
        swipe,
        backgroundContent = { SwipeBackground(swipe.dismissDirection, done = "Complete") },
        onDismiss = { direction ->
            when (direction) {
                SwipeToDismissBoxValue.StartToEnd -> model.changeWork(item, WorkAction.COMPLETE)
                SwipeToDismissBoxValue.EndToStart -> model.changeWork(item, WorkAction.LATER)
                SwipeToDismissBoxValue.Settled -> {}
            }
            scope.launch { swipe.reset() }
        },
    ) {
        WorkRow(item, model.assistNote(item), model.workDetail == item.id) { model.workDetail = item.id }
    }
}

/** The title, then "waiting on decision · work · later", then the newest request's state or "Result ready". */
@Composable
private fun WorkRow(item: WorkItem, assist: String?, focused: Boolean, open: () -> Unit) {
    Column(
        Modifier.fillMaxWidth()
            .background(if (focused) MaterialTheme.colorScheme.secondaryContainer else MaterialTheme.colorScheme.surface)
            .clickable(onClick = open)
            .padding(horizontal = 16.dp, vertical = 10.dp)
            .testTag("work-row-${item.id}"),
    ) {
        Text(item.title, maxLines = 2, overflow = TextOverflow.Ellipsis)
        Text(
            item.subtitle,
            Modifier.testTag("work-state-${item.id}"),
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
        assist?.let {
            Text(
                it,
                Modifier.testTag("work-assist-${item.id}"),
                style = MaterialTheme.typography.labelMedium,
                color = if (it == RESULT_READY) Theme.connected else MaterialTheme.colorScheme.onSurfaceVariant,
                maxLines = 1,
            )
        }
    }
}

/** "open · work · pinned · due Wed 12 Nov 16:00"; pins and deadlines come at `work:10` only. */
val WorkItem.subtitle: String
    get() = listOfNotNull(stateLabel, workspace, "later".takeIf { isLater }, "pinned".takeIf { isPinned }, due?.label()).joinToString(" · ")

/** The first line; then a Work item, a note, or a link to an open item. */
@Composable
private fun CaptureRow(model: AppModel, capture: Capture, work: List<WorkItem>) {
    val busy = model.changingWork != null
    Column(Modifier.fillMaxWidth().padding(start = 16.dp, end = 8.dp, top = 10.dp).testTag("capture-row-${capture.id}")) {
        Text(capture.firstLine, maxLines = 2, overflow = TextOverflow.Ellipsis)
        Text(
            listOfNotNull(AgentSummary.age(capture.createdAt), capture.workspace).joinToString(" · "),
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        Row(horizontalArrangement = Arrangement.spacedBy(4.dp)) {
            TextButton({ model.sort(capture, SortAction.WORK) }, Modifier.testTag("capture-work-${capture.id}"), enabled = !busy) {
                Text("Work item")
            }
            TextButton({ model.sort(capture, SortAction.NOTE) }, Modifier.testTag("capture-note-${capture.id}"), enabled = !busy) {
                Text("Keep as note")
            }
            if (work.isNotEmpty()) LinkButton(model, capture, work, enabled = !busy)
        }
    }
}

@Composable
private fun LinkButton(model: AppModel, capture: Capture, work: List<WorkItem>, enabled: Boolean) {
    var open by remember { mutableStateOf(false) }
    Box {
        TextButton({ open = true }, Modifier.testTag("capture-link-${capture.id}"), enabled = enabled) { Text("Add to…") }
        DropdownMenu(open, { open = false }) {
            for (w in work) {
                DropdownMenuItem({ Text(w.title, maxLines = 1, overflow = TextOverflow.Ellipsis) }, {
                    open = false
                    model.sort(capture, SortAction.LINK, w)
                }, modifier = Modifier.testTag("capture-link-${capture.id}-${w.id}"))
            }
        }
    }
}

/** Kept on the device until the host has it. A refusal shows the host's reason. Delete asks first: it is the only copy. */
@Composable
private fun UnsentRow(model: AppModel, c: UnsentCapture) {
    var deleting by remember { mutableStateOf(false) }
    if (deleting) {
        AlertDialog(
            onDismissRequest = { deleting = false },
            title = { Text("Delete This Capture?") },
            text = { Text("It was never sent, so this device has the only copy.") },
            confirmButton = {
                TextButton({
                    deleting = false
                    model.unsent.remove(c.clientId)
                }, Modifier.testTag("unsent-delete-confirm")) { Text("Delete") }
            },
            dismissButton = { TextButton({ deleting = false }) { Text("Cancel") } },
        )
    }
    Column(Modifier.fillMaxWidth().padding(start = 16.dp, end = 8.dp, top = 10.dp).testTag("unsent-row-${c.clientId}")) {
        Text(firstLine(c.text), maxLines = 2, overflow = TextOverflow.Ellipsis)
        Text(
            c.error?.let { "Not sent: $it" } ?: "Not sent",
            Modifier.testTag("unsent-state-${c.clientId}"),
            style = MaterialTheme.typography.bodySmall,
            color = if (c.error != null) Theme.failure else Theme.attention,
        )
        Row(horizontalArrangement = Arrangement.spacedBy(4.dp)) {
            TextButton({ model.sendNow(c) }, Modifier.testTag("unsent-send-${c.clientId}")) { Text("Send Now") }
            TextButton({ deleting = true }, Modifier.testTag("unsent-delete-${c.clientId}")) { Text("Delete") }
        }
    }
}
