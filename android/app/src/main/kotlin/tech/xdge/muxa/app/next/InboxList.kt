package tech.xdge.muxa.app.next

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.SwipeToDismissBox
import androidx.compose.material3.SwipeToDismissBoxValue
import androidx.compose.material3.Text
import androidx.compose.material3.rememberSwipeToDismissBoxState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlinx.coroutines.launch
import tech.xdge.muxa.app.ui.Icons
import tech.xdge.muxa.app.ui.LocalPhoneLayout
import tech.xdge.muxa.app.ui.Palette
import tech.xdge.muxa.app.ui.Theme
import tech.xdge.muxa.v2.InboxItem
import tech.xdge.muxa.v2.ItemKind
import tech.xdge.muxa.v2.V2Request

/** What the Inbox list shows. */
data class InboxFilter(
    val kind: ItemKind? = null,
    val workspace: String? = null,
    val showLater: Boolean = false,
    val showDone: Boolean = false,
) {
    val narrows: Boolean get() = kind != null || workspace != null
    val any: Boolean get() = narrows || showLater || showDone

    fun shows(item: InboxItem, now: Long) =
        (kind == null || item.kind == kind) && (workspace == null || item.workspace == workspace) && (showLater || !item.isLater(now))
}

/** Filter, Reorder and More, as icons beside the Inbox's title. */
@Composable
fun InboxTools(
    model: NextModel,
    filter: InboxFilter,
    change: (InboxFilter) -> Unit,
    reordering: Boolean,
    reorder: (Boolean) -> Unit,
) {
    var menu by remember { mutableStateOf(false) }
    val workspaces = model.inbox.items.mapNotNull { it.workspace }.toSortedSet()
    Box {
        IconButton({ menu = true }, Modifier.testTag("inbox.filter")) {
            Icon(Icons.filter, if (filter.any) "Filter, on" else "Filter", tint = if (filter.any) Palette.accent else Palette.textSecondary)
        }
        DropdownMenu(menu, { menu = false }) {
            fun pick(f: InboxFilter) {
                menu = false
                change(f)
            }
            DropdownMenuItem({ Text("Every kind") }, { pick(filter.copy(kind = null)) })
            for (k in ItemKind.entries) DropdownMenuItem({ Text(k.title) }, { pick(filter.copy(kind = k)) })
            HorizontalDivider()
            DropdownMenuItem({ Text("Every Workspace") }, { pick(filter.copy(workspace = null)) })
            for (w in workspaces) DropdownMenuItem({ Text(w) }, { pick(filter.copy(workspace = w)) })
            HorizontalDivider()
            DropdownMenuItem({ Text(if (filter.showLater) "Hide Later Items" else "Show Later Items") }, {
                pick(filter.copy(showLater = !filter.showLater))
            })
            DropdownMenuItem({ Text(if (filter.showDone) "Hide Done" else "Show Done") }, {
                pick(filter.copy(showDone = !filter.showDone))
            })
        }
    }
    // Moving items means little in a narrowed list.
    if (!filter.narrows) {
        IconButton({ reorder(!reordering) }, Modifier.testTag("inbox.reorder")) {
            Icon(Icons.reorder, if (reordering) "Done Reordering" else "Reorder", tint = if (reordering) Palette.accent else Palette.textSecondary)
        }
    }
    InboxMoreMenu(model)
}

@Composable
fun InboxList(
    model: NextModel,
    visible: List<InboxItem>,
    filter: InboxFilter,
    now: Long,
    selected: String?,
    reordering: Boolean,
    select: (String) -> Unit,
) {
    val list = rememberLazyListState()
    // A new item goes on top. The list keeps its first row in place on an insert,
    // which would hide the new item above it.
    LaunchedEffect(visible.firstOrNull()?.id) {
        if (list.firstVisibleItemIndex <= 1) list.scrollToItem(0)
    }
    LazyColumn(
        Modifier.fillMaxWidth().testTag("inbox.list"),
        state = list,
        contentPadding = PaddingValues(horizontal = 12.dp, vertical = 6.dp),
        verticalArrangement = Arrangement.spacedBy(if (LocalPhoneLayout.current) 6.dp else 4.dp),
    ) {
        items(visible, key = { it.id }) { item ->
            val at = model.inbox.items.indexOfFirst { it.id == item.id }
            Row(verticalAlignment = Alignment.CenterVertically) {
                SwipeRow(model, item, Modifier.weight(1f)) {
                    ItemRow(item, now, selected = item.id == selected) { select(item.id) }
                }
                if (reordering && !filter.narrows) {
                    IconButton({ model.inbox(V2Request.InboxMove(item.id, at - 1)) }, enabled = at > 0) {
                        Icon(Icons.moveUp, "Move Up")
                    }
                    IconButton({ model.inbox(V2Request.InboxMove(item.id, at + 1)) }, enabled = at < model.inbox.items.size - 1) {
                        Icon(Icons.moveDown, "Move Down")
                    }
                }
            }
        }
        if (visible.isEmpty()) {
            item {
                Text(
                    if (model.inbox.items.isEmpty()) "Nothing to do." else "Nothing matches the filter.",
                    Modifier.padding(16.dp), color = Palette.textSecondary,
                )
            }
        }
        if (filter.showDone && model.inbox.done.isNotEmpty()) {
            item { SectionHeader("Done", Palette.textTertiary, model.inbox.done.size) }
            items(model.inbox.done, key = { "done-" + it.id }) { item ->
                ItemRow(item, now, selected = item.id == selected) { select(item.id) }
            }
        }
    }
}

/** A list section's header: small capitals in the section's color, and a count. */
@Composable
fun SectionHeader(title: String, color: Color, count: Int? = null) {
    Row(Modifier.padding(start = 6.dp, end = 6.dp, top = 10.dp, bottom = 4.dp), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
        Text(title.uppercase(), color = color, fontSize = 11.5.sp, fontWeight = FontWeight.SemiBold, letterSpacing = 0.7.sp)
        count?.let { Text("$it", color = Palette.textTertiary, fontSize = 11.5.sp, fontWeight = FontWeight.SemiBold) }
    }
}

/** Swipe left for Done, right for Later. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun SwipeRow(model: NextModel, item: InboxItem, modifier: Modifier, content: @Composable () -> Unit) {
    val state = rememberSwipeToDismissBoxState()
    val scope = rememberCoroutineScope()
    SwipeToDismissBox(
        state,
        modifier = modifier.clip(RoundedCornerShape(14.dp)),
        onDismiss = { value ->
            when (value) {
                SwipeToDismissBoxValue.EndToStart -> model.inbox(V2Request.InboxDone(item.id))
                SwipeToDismissBoxValue.StartToEnd -> model.inbox(V2Request.InboxLater(item.id, LaterChoice.TOMORROW.until()))
                SwipeToDismissBoxValue.Settled -> {}
            }
            // The row leaves when the Inbox changes; until then it stays in place.
            scope.launch { state.reset() }
        },
        backgroundContent = {
            val later = state.dismissDirection == SwipeToDismissBoxValue.StartToEnd
            Row(
                Modifier.fillMaxSize().background(if (later) Theme.later else Theme.done).padding(horizontal = 20.dp),
                horizontalArrangement = if (later) Arrangement.Start else Arrangement.End,
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Icon(if (later) Icons.later else Icons.check, null, tint = Color.White)
                Spacer(Modifier.width(8.dp))
                Text(if (later) "Later" else "Done", color = Color.White, fontWeight = FontWeight.SemiBold, fontSize = 13.sp)
            }
        },
    ) { content() }
}
