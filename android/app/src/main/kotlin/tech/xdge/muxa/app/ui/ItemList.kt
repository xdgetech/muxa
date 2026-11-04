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
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.SwipeToDismissBox
import androidx.compose.material3.SwipeToDismissBoxValue
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.rememberSwipeToDismissBoxState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.launch
import tech.xdge.muxa.app.AppModel
import tech.xdge.muxa.app.InboxPage
import tech.xdge.muxa.app.InboxPrompt
import tech.xdge.muxa.app.openItem
import tech.xdge.muxa.protocol.InboxAction
import tech.xdge.muxa.protocol.InboxItem
import tech.xdge.muxa.session.isUnread

/**
 * The Inbox as a list of questions (ADR 0033), like a mail list: the question, then where it
 * comes from and its age. A dot marks unread. The host's order is kept. A swipe asks for Later.
 */
@Composable
fun ItemList(model: AppModel, modifier: Modifier) {
    val items = model.inboxItems
    val ids = items.map { it.id }
    // The focused item leaves the list when it is answered, here or on another device.
    LaunchedEffect(ids) { if (model.itemFocus !in ids) model.itemFocus = ids.firstOrNull() }
    LazyColumn(modifier.fillMaxWidth().testTag("inbox-list")) {
        if (!model.usesItems) {
            item(key = "old-host") { OlderHost(model) }
        } else if (items.isEmpty()) {
            item(key = "empty") {
                Column(Modifier.fillMaxWidth().padding(32.dp).testTag("inbox-empty"), horizontalAlignment = Alignment.CenterHorizontally) {
                    Text("Nothing needs you", style = MaterialTheme.typography.titleMedium)
                    TextButton(
                        {
                            model.clearInboxSelection()
                            model.inboxPage = InboxPage.TASKS
                        },
                        Modifier.testTag("inbox-all-tasks"),
                    ) { Text("All Tasks") }
                }
            }
        }
        items(items, key = { it.id }) { item ->
            ItemRowView(model, item, item.id == model.selectedItem || (model.selectedItem == null && item.id == model.itemFocus))
            HorizontalDivider()
        }
    }
}

/** A host below `work` level 3 keeps no Inbox items (ADR 0033): say so, and offer the other lists. */
@Composable
private fun OlderHost(model: AppModel) {
    Column(Modifier.fillMaxWidth().padding(32.dp).testTag("inbox-old-host"), horizontalAlignment = Alignment.CenterHorizontally) {
        Text(OLDER_HOST, style = MaterialTheme.typography.bodyMedium, textAlign = TextAlign.Center)
        TextButton(
            {
                model.clearInboxSelection()
                model.inboxPage = InboxPage.TASKS
            },
            Modifier.testTag("inbox-all-tasks"),
        ) { Text("All Tasks") }
        TextButton(model::closeTasks, Modifier.testTag("inbox-sessions")) { Text("Sessions") }
    }
}

/** What the Inbox says on a host below `work` level 3. */
const val OLDER_HOST = "This host needs a newer muxa-bridge for the Inbox. Update muxa on the host."

/** Either swipe asks for Later, where the host takes it now. */
@Composable
private fun ItemRowView(model: AppModel, item: InboxItem, focused: Boolean) {
    val swipe = rememberSwipeToDismissBoxState()
    val scope = rememberCoroutineScope()
    val canLater = InboxAction.LATER in item.answers
    SwipeToDismissBox(
        swipe,
        backgroundContent = { LaterBackground(swipe.dismissDirection) },
        enableDismissFromStartToEnd = canLater,
        enableDismissFromEndToStart = canLater,
        onDismiss = { direction ->
            if (direction != SwipeToDismissBoxValue.Settled) model.inboxPrompt = InboxPrompt.ItemLater(item)
            scope.launch { swipe.reset() }
        },
    ) {
        ItemRow(item, focused) { model.openItem(item.id) }
    }
}

@Composable
private fun LaterBackground(direction: SwipeToDismissBoxValue) {
    val alignment = when (direction) {
        SwipeToDismissBoxValue.StartToEnd -> Alignment.CenterStart
        SwipeToDismissBoxValue.EndToStart -> Alignment.CenterEnd
        SwipeToDismissBoxValue.Settled -> return
    }
    Box(Modifier.fillMaxSize().background(MaterialTheme.colorScheme.secondaryContainer).padding(horizontal = 20.dp), contentAlignment = alignment) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Icon(Icons.later, null)
            Text("Later")
        }
    }
}

/** [tag] names the row for tests; the sidebar's copy has its own. */
@Composable
fun ItemRow(item: InboxItem, focused: Boolean, tag: String = "item-row-${item.id}", open: () -> Unit) {
    val unread = item.isUnread
    Row(
        Modifier.fillMaxWidth()
            .background(if (focused) MaterialTheme.colorScheme.secondaryContainer else MaterialTheme.colorScheme.surface)
            .clickable(onClick = open)
            .padding(horizontal = 16.dp, vertical = 10.dp)
            .semantics { stateDescription = if (unread) "unread" else "read" }
            .testTag(tag),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Box(Modifier.size(8.dp).background(if (unread) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.surface, CircleShape))
        Column(Modifier.weight(1f)) {
            Text(
                item.question,
                fontWeight = if (unread) FontWeight.SemiBold else FontWeight.Normal,
                maxLines = 2,
                overflow = TextOverflow.Ellipsis,
            )
            Text(
                item.source,
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
    }
}

/** "Fix the login redirect · 10m": the Task title, or the Routine, and the age. */
val InboxItem.source: String get() = "${routine ?: title} · ${AgentSummary.age(createdAt)}"
