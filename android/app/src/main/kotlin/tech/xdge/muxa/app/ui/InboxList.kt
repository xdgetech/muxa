package tech.xdge.muxa.app.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.SwipeToDismissBox
import androidx.compose.material3.SwipeToDismissBoxValue
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.rememberSwipeToDismissBoxState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import kotlinx.coroutines.launch
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import tech.xdge.muxa.app.AppModel
import tech.xdge.muxa.app.InboxPrompt
import tech.xdge.muxa.app.act
import tech.xdge.muxa.app.dismiss
import tech.xdge.muxa.app.done
import tech.xdge.muxa.app.moveDown
import tech.xdge.muxa.app.snooze
import tech.xdge.muxa.protocol.InboxEntry
import tech.xdge.muxa.session.InboxRow
import tech.xdge.muxa.session.hasKnownActions
import tech.xdge.muxa.session.isProposal
import tech.xdge.muxa.session.key
import tech.xdge.muxa.session.need

/**
 * The Inbox as one ordered list (ADR 0025 "List first"): Task needs in the host's order,
 * waiting Routine runs, briefings and proposals. A tap opens the row (Act); its menu and a
 * swipe have Done, Move down and Later where they apply.
 */
@Composable
fun InboxList(model: AppModel, modifier: Modifier) {
    val rows = model.inboxRows
    val ids = rows.map { it.id }
    // The focused row may leave the list: Move down, Later, Done.
    LaunchedEffect(ids) { if (model.inboxFocus !in ids) model.inboxFocus = ids.firstOrNull() }
    LazyColumn(modifier.fillMaxWidth().testTag("inbox-list")) {
        if (rows.isEmpty()) {
            item(key = "empty") {
                Column(Modifier.fillMaxWidth().padding(32.dp).testTag("inbox-empty"), horizontalAlignment = Alignment.CenterHorizontally) {
                    Text("Nothing needs you", style = MaterialTheme.typography.titleMedium)
                    Text(
                        "All Tasks keeps every Task, running and closed.",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
            }
        }
        items(rows, key = { it.id }) { row -> InboxRowView(model, row, row.id == model.inboxFocus) }
        val snoozed = model.inboxSnoozedCount
        if (snoozed > 0) {
            item(key = "snoozed") {
                TextButton({ model.inboxSnoozes = emptyMap() }, Modifier.padding(horizontal = 8.dp).testTag("inbox-show-snoozed")) {
                    Text("Show snoozed ($snoozed)")
                }
            }
        }
    }
}

/** Swipe right asks for Later; swipe left moves a Task down. Each only where the host keeps it. */
@Composable
private fun InboxRowView(model: AppModel, row: InboxRow, focused: Boolean) {
    val canLater = when (row) {
        is InboxRow.Task -> model.canPlaceTasks
        is InboxRow.Entry -> row.entry.hasKnownActions
        is InboxRow.Routine -> false
    }
    val canMoveDown = row is InboxRow.Task && model.canPlaceTasks
    val swipe = rememberSwipeToDismissBoxState()
    val scope = rememberCoroutineScope()
    SwipeToDismissBox(
        swipe,
        backgroundContent = { SwipeBackground(swipe.dismissDirection) },
        enableDismissFromStartToEnd = canLater,
        enableDismissFromEndToStart = canMoveDown,
        onDismiss = { direction ->
            when (direction) {
                SwipeToDismissBoxValue.StartToEnd -> model.inboxPrompt = InboxPrompt.Later(row)
                SwipeToDismissBoxValue.EndToStart -> model.moveDown(row)
                SwipeToDismissBoxValue.Settled -> {}
            }
            scope.launch { swipe.reset() }
        },
    ) {
        RowContent(model, row, focused)
    }
}

@Composable
private fun SwipeBackground(direction: SwipeToDismissBoxValue) {
    val (label, icon, alignment) = when (direction) {
        SwipeToDismissBoxValue.StartToEnd -> Triple("Later", Icons.later, Alignment.CenterStart)
        SwipeToDismissBoxValue.EndToStart -> Triple("Move Down", Icons.moveDown, Alignment.CenterEnd)
        SwipeToDismissBoxValue.Settled -> return
    }
    Box(Modifier.fillMaxSize().background(MaterialTheme.colorScheme.secondaryContainer).padding(horizontal = 20.dp), contentAlignment = alignment) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Icon(icon, null)
            Text(label)
        }
    }
}

@Composable
private fun RowContent(model: AppModel, row: InboxRow, focused: Boolean) {
    var menu by remember { mutableStateOf(false) }
    Row(
        Modifier.fillMaxWidth()
            .background(if (focused) MaterialTheme.colorScheme.secondaryContainer else MaterialTheme.colorScheme.surface)
            .padding(start = 16.dp, end = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Row(
            Modifier.weight(1f).clickable { model.act(row) }.padding(vertical = 10.dp).testTag("inbox-row-${row.id}"),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            Icon(row.icon, null, tint = MaterialTheme.colorScheme.onSurfaceVariant)
            Column {
                Text(row.title, fontWeight = FontWeight.SemiBold, maxLines = 2, overflow = TextOverflow.Ellipsis)
                Text(row.subtitle, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant, maxLines = 2)
            }
        }
        Box {
            IconButton({ menu = true }, Modifier.testTag("inbox-actions-${row.id}")) { Icon(Icons.more, "Actions") }
            DropdownMenu(menu, { menu = false }) { RowMenu(model, row) { menu = false } }
        }
    }
}

/** Act, Done, Move down and Later, as the row and the host allow. */
@Composable
private fun RowMenu(model: AppModel, row: InboxRow, close: () -> Unit) {
    fun pick(action: () -> Unit) {
        close()
        action()
    }
    @Composable
    fun item(title: String, icon: ImageVector, tag: String, action: () -> Unit) =
        DropdownMenuItem({ Text(title) }, { pick(action) }, leadingIcon = { Icon(icon, null) }, modifier = Modifier.testTag(tag))
    item(if (row is InboxRow.Routine) "Open Terminal" else "Open", if (row is InboxRow.Routine) Icons.agent else row.icon, "inbox-open") {
        model.act(row)
    }
    when (row) {
        is InboxRow.Task -> {
            if (model.canPlaceTasks) {
                item("Move Down", Icons.moveDown, "inbox-move-down") { model.moveDown(row) }
                item("Later…", Icons.later, "inbox-later") { model.inboxPrompt = InboxPrompt.Later(row) }
            } else {
                item("Snooze", Icons.later, "inbox-snooze") { model.snooze(row.item) }
            }
            item("Mark Done", Icons.check, "inbox-done") { model.done(row) }
        }
        is InboxRow.Entry -> if (row.entry.hasKnownActions) {
            if (row.entry.isProposal) {
                item("Approve…", Icons.done, "inbox-approve") { model.inboxPrompt = InboxPrompt.Approve(row.entry) }
                item("Dismiss", Icons.close, "inbox-dismiss") { model.dismiss(row.entry) }
            } else {
                item("Done", Icons.check, "inbox-done") { model.done(row) }
            }
            item("Later…", Icons.later, "inbox-later") { model.inboxPrompt = InboxPrompt.Later(row) }
        }
        is InboxRow.Routine -> item("Routine", Icons.routine, "inbox-routine") {
            model.clearInboxSelection()
            model.selectedRoutine = row.need.routine.key
        }
    }
}

val InboxRow.title: String
    get() = when (this) {
        is InboxRow.Task -> item.record.title
        is InboxRow.Entry -> entry.title
        is InboxRow.Routine -> need.routine.name
    }

/** What the row needs, and where it comes from. */
val InboxRow.subtitle: String
    get() = when (this) {
        is InboxRow.Task -> listOfNotNull(item.need.title, item.record.workspace, item.record.project).joinToString(" · ")
        is InboxRow.Entry -> "${entry.kindTitle} · ${entry.workspace}/${entry.routine}"
        is InboxRow.Routine -> "Routine run waits for your answer · ${need.routine.workspace}"
    }

val InboxRow.icon: ImageVector
    get() = when (this) {
        is InboxRow.Task -> Icons.tasks
        is InboxRow.Entry -> if (entry.isProposal) Icons.proposal else Icons.briefing
        is InboxRow.Routine -> Icons.routine
    }

val InboxEntry.kindTitle: String
    get() = when (kind) {
        "briefing" -> "Briefing"
        "proposal" -> "Proposal"
        else -> kind
    }
