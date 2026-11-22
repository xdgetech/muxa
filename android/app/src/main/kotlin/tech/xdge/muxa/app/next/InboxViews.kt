package tech.xdge.muxa.app.next

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Card
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import tech.xdge.muxa.app.ui.Icons
import tech.xdge.muxa.app.ui.Theme
import tech.xdge.muxa.v2.InboxItem
import tech.xdge.muxa.v2.ItemKind
import tech.xdge.muxa.v2.Terminal
import tech.xdge.muxa.v2.V2Request
import java.time.DayOfWeek
import java.time.ZonedDateTime
import java.time.temporal.TemporalAdjusters

val ItemKind.title: String
    get() = when (this) {
        ItemKind.REVIEW -> "Review"
        ItemKind.REPLY -> "Reply"
        ItemKind.DECIDE -> "Decide"
        ItemKind.READ -> "Read"
        ItemKind.TODO -> "To do"
        ItemKind.FAILED -> "Failed"
    }

val ItemKind.tint: Color
    get() = when (this) {
        ItemKind.REVIEW -> Theme.accent
        ItemKind.REPLY -> Color(0xFF5E5CE6)
        ItemKind.DECIDE -> Theme.attention
        ItemKind.READ -> Color(0xFF30B0C7)
        ItemKind.TODO -> Color(0xFF8E8E93)
        ItemKind.FAILED -> Theme.failure
    }

/** The action Ctrl+Enter takes on an item of this kind. */
val ItemKind.mainAction: String
    get() = when (this) {
        ItemKind.REVIEW -> "Approve"
        ItemKind.FAILED -> "Retry"
        else -> "Done"
    }

@Composable
fun KindLabel(kind: ItemKind) {
    Text(kind.title, style = MaterialTheme.typography.labelMedium, fontWeight = FontWeight.SemiBold, color = kind.tint)
}

/** A due date; red once it has passed. */
@Composable
fun DueLabel(due: String) {
    val today = NextModel.today()
    val overdue = due < today
    Text(
        if (due == today) "Today" else due,
        style = MaterialTheme.typography.labelMedium,
        color = if (overdue) Theme.failure else MaterialTheme.colorScheme.onSurfaceVariant,
        modifier = Modifier.semantics { contentDescription = if (overdue) "Overdue $due" else "Due $due" },
    )
}

@Composable
fun ItemRow(item: InboxItem, now: Long, modifier: Modifier = Modifier) {
    Column(
        modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 10.dp).testTag("item.${item.id}"),
        verticalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
            KindLabel(item.kind)
            item.workspace?.let { Text(it, style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant) }
            Spacer(Modifier.weight(1f))
            if (item.isLater(now)) Icon(Icons.later, "Later", Modifier.size(16.dp), tint = MaterialTheme.colorScheme.onSurfaceVariant)
            item.due?.let { DueLabel(it) }
        }
        Text(item.title, maxLines = 2, overflow = TextOverflow.Ellipsis)
    }
}

/** When a Later item comes back. */
enum class LaterChoice(val title: String) {
    HOUR("In 1 Hour"),
    TOMORROW("Tomorrow 9:00"),
    NEXT_WEEK("Next Monday 9:00"),
    ;

    /** Unix seconds. */
    fun until(now: ZonedDateTime = ZonedDateTime.now()): Long = when (this) {
        HOUR -> now.plusHours(1)
        TOMORROW -> now.plusDays(1).withHour(9).withMinute(0).withSecond(0).withNano(0)
        NEXT_WEEK -> now.with(TemporalAdjusters.next(DayOfWeek.MONDAY)).withHour(9).withMinute(0).withSecond(0).withNano(0)
    }.toEpochSecond()
}

/**
 * The wide pane: what the item asks, and its actions. [position] is where the
 * item stands in the user's order, from 1; null when done.
 */
@Composable
fun InboxItemPane(
    model: NextModel,
    item: InboxItem,
    position: Int?,
    actions: ItemActionState,
    openTerminal: (Terminal) -> Unit,
    newTask: (TaskSeed) -> Unit,
    onDone: () -> Unit,
) {
    Column(
        Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(16.dp).widthIn(max = 760.dp),
        verticalArrangement = Arrangement.spacedBy(16.dp),
    ) {
        Row(horizontalArrangement = Arrangement.spacedBy(12.dp), verticalAlignment = Alignment.CenterVertically) {
            KindLabel(item.kind)
            val secondary = MaterialTheme.colorScheme.onSurfaceVariant
            position?.let { Text("#$it", style = MaterialTheme.typography.labelMedium, color = secondary) }
            Text(source(item), style = MaterialTheme.typography.labelMedium, color = secondary)
            item.workspace?.let { Text(it, style = MaterialTheme.typography.labelMedium, color = secondary) }
            item.due?.let { DueLabel(it) }
        }
        Text(item.title, style = MaterialTheme.typography.titleLarge, modifier = Modifier.testTag("detail.title"))
        item.note?.let { note ->
            Card(Modifier.fillMaxWidth()) {
                Column(Modifier.padding(12.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                    Text("Summary", style = MaterialTheme.typography.titleSmall)
                    SelectionContainer { Text(note) }
                }
            }
        }
        if (position == null) {
            OutlinedButton({ model.inbox(V2Request.InboxReopen(item.id)) }) { Text("Reopen") }
        } else {
            ItemActions(model, item, actions, openTerminal, newTask, onDone)
        }
        model.requestError?.let { Text(it, Modifier.testTag("detail.error"), color = Theme.failure) }
    }
}

private fun source(item: InboxItem) = when (item.source) {
    "task" -> "From a Task"
    "routine" -> "From a Routine"
    else -> "Captured"
}
