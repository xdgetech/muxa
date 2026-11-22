package tech.xdge.muxa.app.next

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import java.time.DayOfWeek
import java.time.LocalDate
import java.time.ZonedDateTime
import java.time.format.DateTimeFormatter
import java.time.format.TextStyle
import java.time.temporal.ChronoUnit
import java.time.temporal.TemporalAdjusters
import java.util.Locale
import tech.xdge.muxa.app.ui.Icons
import tech.xdge.muxa.app.ui.KindColors
import tech.xdge.muxa.app.ui.KindPalette
import tech.xdge.muxa.app.ui.LocalPhoneLayout
import tech.xdge.muxa.app.ui.Palette
import tech.xdge.muxa.app.ui.SecondaryButton
import tech.xdge.muxa.app.ui.Theme
import tech.xdge.muxa.v2.InboxItem
import tech.xdge.muxa.v2.ItemKind
import tech.xdge.muxa.v2.Terminal
import tech.xdge.muxa.v2.V2Request

val ItemKind.title: String
    get() = when (this) {
        ItemKind.REVIEW -> "Review"
        ItemKind.REPLY -> "Reply"
        ItemKind.DECIDE -> "Decide"
        ItemKind.READ -> "Read"
        ItemKind.TODO -> "To do"
        ItemKind.FAILED -> "Failed"
    }

/** The kind's chip colors, as the design's `K`. To do is neutral. */
val ItemKind.colors: KindColors
    get() = when (this) {
        ItemKind.REVIEW -> KindPalette.review
        ItemKind.FAILED -> KindPalette.failed
        ItemKind.REPLY -> KindPalette.reply
        ItemKind.DECIDE -> KindPalette.decide
        ItemKind.READ -> KindPalette.read
        ItemKind.TODO -> KindPalette.todo
    }

val ItemKind.tint: Color get() = colors.content

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

/** A due date: "today", a weekday this week, or the date; red once it has passed. */
@Composable
fun DueLabel(due: String) {
    val today = NextModel.today()
    val overdue = due < today
    Text(
        dueText(due, today),
        fontSize = 12.5.sp,
        color = if (overdue) Theme.failure else Palette.textSecondary,
        maxLines = 1,
        modifier = Modifier.semantics { contentDescription = if (overdue) "Overdue $due" else "Due $due" },
    )
}

/** [due] and [today] are `YYYY-MM-DD`. */
fun dueText(due: String, today: String): String {
    if (due == today) return "today"
    val (d, t) = runCatching { LocalDate.parse(due) to LocalDate.parse(today) }.getOrNull() ?: return due
    val days = ChronoUnit.DAYS.between(t, d)
    return when {
        days in 1..WEEK_DAYS -> d.dayOfWeek.getDisplayName(TextStyle.SHORT, Locale.getDefault())
        d.year == t.year -> d.format(DateTimeFormatter.ofPattern("MMM d", Locale.getDefault()))
        else -> due
    }
}

/** Where an item came from: its Workspace, and the Routine, Task or capture. */
fun whereFrom(item: InboxItem): String = listOfNotNull(
    item.workspace,
    item.routine?.let { "Routine · ${it.name}" } ?: when (item.source) {
        "task" -> "Task"
        "routine" -> "Routine"
        else -> "captured"
    },
).joinToString(" · ")

/** An Inbox item as the design's card: the kind's chip, the title, where it came from and when it is due. */
@Composable
fun ItemRow(item: InboxItem, now: Long, modifier: Modifier = Modifier, selected: Boolean = false, onClick: (() -> Unit)? = null) {
    val clickable = if (onClick != null) Modifier.clickable(onClick = onClick) else Modifier
    // The tablet's sidebar is denser, as the design's rows there.
    val compact = !LocalPhoneLayout.current
    Row(
        modifier
            .fillMaxWidth()
            .clip(CARD)
            .background(if (selected) Palette.picked else Palette.raised)
            .semantics { this.selected = selected }
            .then(clickable)
            .padding(if (compact) 10.dp else 12.dp)
            .testTag("item.${item.id}"),
        horizontalArrangement = Arrangement.spacedBy(if (compact) 10.dp else 12.dp),
    ) {
        KindChip(item.kind, Modifier.width(56.dp).padding(top = 1.dp))
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(if (compact) 2.dp else 3.dp)) {
            Text(
                item.title, fontSize = if (compact) 14.sp else 15.sp, lineHeight = if (compact) 19.sp else 20.sp,
                maxLines = 2, overflow = TextOverflow.Ellipsis,
            )
            Row(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalAlignment = Alignment.CenterVertically) {
                Text(whereFrom(item), Modifier.weight(1f), fontSize = 12.5.sp, color = Palette.textSecondary,
                    maxLines = 1, overflow = TextOverflow.Ellipsis)
                if (item.isLater(now)) Icon(Icons.later, "Later", Modifier.size(14.dp), tint = Palette.textSecondary)
                item.due?.let { DueLabel(it) }
            }
        }
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
        Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(horizontal = 24.dp, vertical = 20.dp).widthIn(max = 760.dp),
        verticalArrangement = Arrangement.spacedBy(16.dp),
    ) {
        Row(horizontalArrangement = Arrangement.spacedBy(10.dp), verticalAlignment = Alignment.CenterVertically) {
            KindChip(item.kind, Modifier.width(56.dp))
            val meta = listOfNotNull(position?.let { "#$it" }, source(item), item.workspace).joinToString(" · ")
            Text(meta, Modifier.weight(1f, fill = false), fontSize = 13.sp, color = Palette.textSecondary, maxLines = 1)
            item.due?.let { DueLabel(it) }
        }
        Text(item.title, fontSize = 22.sp, lineHeight = 28.sp, fontWeight = FontWeight.SemiBold, modifier = Modifier.testTag("detail.title"))
        item.note?.let { note ->
            Column(
                Modifier.fillMaxWidth().background(Palette.surface, RoundedCornerShape(10.dp))
                    .border(1.dp, Palette.border, RoundedCornerShape(10.dp)).padding(14.dp),
                verticalArrangement = Arrangement.spacedBy(6.dp),
            ) {
                Text("Summary", fontSize = 12.sp, color = Palette.textTertiary)
                SelectionContainer { Text(note, color = Palette.text, lineHeight = 22.sp) }
            }
        }
        if (position == null) {
            SecondaryButton("Reopen", { model.inbox(V2Request.InboxReopen(item.id)) })
        } else {
            ItemActions(model, item, actions, openTerminal, newTask, onDone)
        }
        model.requestError?.let { Text(it, Modifier.testTag("detail.error"), color = Theme.failure) }
    }
}

private fun source(item: InboxItem) = when (item.source) {
    "task" -> "From a Task"
    "routine" -> "From a Routine"
    "v1" -> "From the old muxa"
    else -> "Captured"
}

private val CARD = RoundedCornerShape(14.dp)
private const val WEEK_DAYS = 6L
