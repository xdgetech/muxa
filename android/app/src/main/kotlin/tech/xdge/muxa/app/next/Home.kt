package tech.xdge.muxa.app.next

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.SegmentedButton
import androidx.compose.material3.SegmentedButtonDefaults
import androidx.compose.material3.SingleChoiceSegmentedButtonRow
import androidx.compose.material3.SwipeToDismissBox
import androidx.compose.material3.SwipeToDismissBoxValue
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.VerticalDivider
import androidx.compose.material3.rememberSwipeToDismissBoxState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.isAltPressed
import androidx.compose.ui.input.key.isCtrlPressed
import androidx.compose.ui.input.key.isShiftPressed
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import tech.xdge.muxa.app.ui.Icons
import tech.xdge.muxa.app.ui.LocalPhoneLayout
import tech.xdge.muxa.app.ui.Theme
import tech.xdge.muxa.v2.InboxItem
import tech.xdge.muxa.v2.ItemKind
import tech.xdge.muxa.v2.Terminal
import tech.xdge.muxa.v2.V2Request

private enum class Part(val title: String) { INBOX("Inbox"), ROUTINES("Routines") }

/** What the Inbox list shows. */
private data class InboxFilter(
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

/**
 * Home (ADR 0066): the Inbox and the Routines, a narrow list and a wide detail.
 * In the phone layout the list and the detail are two screens.
 */
@Composable
fun Home(model: NextModel, openTerminal: (Terminal) -> Unit, showFocus: () -> Unit) {
    var part by rememberSaveable { mutableStateOf(Part.INBOX) }
    var selectedItem by rememberSaveable { mutableStateOf<String?>(null) }
    var selectedRoutine by rememberSaveable { mutableStateOf<String?>(null) }
    var filter by remember { mutableStateOf(InboxFilter()) }
    var reordering by remember { mutableStateOf(false) }
    var now by remember { mutableLongStateOf(nowSeconds()) }
    val capture = remember { FocusRequester() }
    val phone = LocalPhoneLayout.current

    // Later items come back on their own; the clock decides what shows.
    LaunchedEffect(Unit) {
        while (true) {
            now = nowSeconds()
            delay(CLOCK_MS)
        }
    }

    val visible = model.inbox.items.filter { filter.shows(it, now) }
    fun nextVisible(after: String): String? {
        val at = visible.indexOfFirst { it.id == after }
        if (at < 0) return null
        return visible.getOrNull(at + 1)?.id ?: visible.getOrNull(at - 1)?.id
    }

    val selected = selectedItem?.let { id -> (model.inbox.items + model.inbox.done).firstOrNull { it.id == id } }
    fun act(r: V2Request) {
        val id = selectedItem ?: return
        model.inbox(r)
        selectedItem = nextVisible(id)
    }

    fun step(by: Int) {
        val id = selectedItem ?: return
        val at = model.inbox.items.indexOfFirst { it.id == id }
        val to = at + by
        if (at >= 0 && to in model.inbox.items.indices) model.inbox(V2Request.InboxMove(id, to))
    }

    val keys = Modifier.onPreviewKeyEvent { e ->
        if (e.type != KeyEventType.KeyDown || e.isShiftPressed) return@onPreviewKeyEvent false
        val item = selected?.takeIf { part == Part.INBOX && it.id in model.inbox.items.map(InboxItem::id) }
        when {
            e.isCtrlPressed && e.key == Key.N -> capture.requestFocus().let { true }
            e.isAltPressed && e.key == Key.DirectionUp && item != null -> step(-1).let { true }
            e.isAltPressed && e.key == Key.DirectionDown && item != null -> step(1).let { true }
            e.isCtrlPressed && e.key == Key.Enter && item != null -> act(V2Request.InboxDone(item.id)).let { true }
            e.isCtrlPressed && e.key == Key.Backspace && item != null -> act(V2Request.InboxDone(item.id)).let { true }
            e.isCtrlPressed && e.key == Key.L && item != null -> act(V2Request.InboxLater(item.id, LaterChoice.TOMORROW.until())).let { true }
            else -> false
        }
    }

    val list: @Composable (Modifier) -> Unit = { modifier ->
        Column(modifier, verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Column(Modifier.padding(horizontal = 16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                ModeBar(model, showFocus)
                CaptureField(model, capture)
                NeedsYouStrip(model, openTerminal)
                SingleChoiceSegmentedButtonRow(Modifier.fillMaxWidth().testTag("home.part")) {
                    Part.entries.forEachIndexed { i, p ->
                        SegmentedButton(
                            part == p, { part = p }, SegmentedButtonDefaults.itemShape(i, Part.entries.size),
                            Modifier.testTag("home.part.${p.title}"),
                        ) { Text(p.title) }
                    }
                }
                if (part == Part.INBOX) {
                    InboxBar(model, filter, { filter = it }, reordering) { reordering = it }
                }
            }
            when (part) {
                Part.INBOX -> InboxList(model, visible, filter, now, selectedItem, reordering) { selectedItem = it }
                Part.ROUTINES -> RoutineList(model, selectedRoutine) { selectedRoutine = it }
            }
            LinkBanner(model)
        }
    }
    val detail: @Composable (Modifier) -> Unit = { modifier ->
        Box(modifier) {
            when (part) {
                Part.INBOX -> if (selected != null) {
                    val position = model.inbox.items.indexOfFirst { it.id == selected.id }.takeIf { it >= 0 }?.plus(1)
                    InboxItemPane(model, selected, position) { selectedItem = nextVisible(selected.id) }
                } else {
                    Empty(if (model.inbox.items.isEmpty()) "Inbox zero" else "Select an item")
                }
                Part.ROUTINES -> model.routines.firstOrNull { it.id == selectedRoutine }?.let { RoutinePane(model, it) }
                    ?: Empty("Select a Routine")
            }
        }
    }

    val shown = if (part == Part.INBOX) selectedItem else selectedRoutine
    Box(Modifier.fillMaxSize().then(keys)) {
        if (phone) {
            if (shown == null) {
                list(Modifier.fillMaxSize())
            } else {
                BackHandler { if (part == Part.INBOX) selectedItem = null else selectedRoutine = null }
                Column(Modifier.fillMaxSize()) {
                    IconButton({ if (part == Part.INBOX) selectedItem = null else selectedRoutine = null }, Modifier.testTag("detail.back")) {
                        Icon(Icons.back, "Back")
                    }
                    detail(Modifier.fillMaxSize())
                }
            }
        } else {
            Row(Modifier.fillMaxSize()) {
                list(Modifier.width(LIST_WIDTH.dp).fillMaxHeight())
                VerticalDivider()
                detail(Modifier.weight(1f).fillMaxHeight())
            }
        }
    }
}

/** The host and the way to Focus: always shown, also in a narrow list. */
@Composable
private fun ModeBar(model: NextModel, showFocus: () -> Unit) {
    var menu by remember { mutableStateOf(false) }
    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
        Box {
            TextButton({ menu = true }, Modifier.testTag("home.host")) {
                Icon(Icons.host, null)
                Text(" " + if (model.demo) "Demo" else model.currentHost?.title ?: "Host")
            }
            DropdownMenu(menu, { menu = false }) {
                DropdownMenuItem({ Text("Disconnect") }, {
                    menu = false
                    model.disconnect()
                }, leadingIcon = { Icon(Icons.disconnect, null) }, modifier = Modifier.testTag("home.disconnect"))
            }
        }
        Box(Modifier.weight(1f))
        TextButton(showFocus, Modifier.testTag("mode.focus")) {
            Icon(Icons.agent, null)
            Text(" Focus")
        }
    }
}

@Composable
private fun InboxBar(
    model: NextModel,
    filter: InboxFilter,
    change: (InboxFilter) -> Unit,
    reordering: Boolean,
    reorder: (Boolean) -> Unit,
) {
    var menu by remember { mutableStateOf(false) }
    val workspaces = model.inbox.items.mapNotNull { it.workspace }.toSortedSet()
    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
        if (!filter.narrows) {
            TextButton({ reorder(!reordering) }, Modifier.testTag("inbox.reorder")) { Text(if (reordering) "Done" else "Reorder") }
        }
        Box(Modifier.weight(1f))
        Box {
            TextButton({ menu = true }, Modifier.testTag("inbox.filter")) { Text(if (filter.any) "Filter •" else "Filter") }
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
    }
}

@Composable
private fun InboxList(
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
    LazyColumn(Modifier.fillMaxWidth().testTag("inbox.list"), state = list) {
        items(visible, key = { it.id }) { item ->
            val at = model.inbox.items.indexOfFirst { it.id == item.id }
            SwipeRow(model, item) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    ItemRow(
                        item, now,
                        Modifier.weight(1f).selectable(item.id == selected) { select(item.id) },
                    )
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
        }
        if (visible.isEmpty()) {
            item {
                Text(
                    if (model.inbox.items.isEmpty()) "Nothing to do." else "Nothing matches the filter.",
                    Modifier.padding(16.dp), color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
        }
        if (filter.showDone && model.inbox.done.isNotEmpty()) {
            item { Text("Done", Modifier.padding(16.dp), style = MaterialTheme.typography.titleSmall) }
            items(model.inbox.done, key = { "done-" + it.id }) { item ->
                ItemRow(item, now, Modifier.selectable(item.id == selected) { select(item.id) })
            }
        }
    }
}

/** Swipe left for Done, right for Later. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun SwipeRow(model: NextModel, item: InboxItem, content: @Composable () -> Unit) {
    val state = rememberSwipeToDismissBoxState()
    val scope = rememberCoroutineScope()
    SwipeToDismissBox(
        state,
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
            Box(
                Modifier.fillMaxSize().background(if (later) Theme.attention else Theme.connected).padding(horizontal = 16.dp),
                contentAlignment = if (later) Alignment.CenterStart else Alignment.CenterEnd,
            ) { Text(if (later) "Later" else "Done", color = Color.White) }
        },
    ) {
        Box(Modifier.background(MaterialTheme.colorScheme.surface)) { content() }
    }
}

@Composable
private fun RoutineList(model: NextModel, selected: String?, select: (String) -> Unit) {
    LaunchedEffect(Unit) { model.refreshRoutines() }
    LazyColumn(Modifier.fillMaxWidth()) {
        items(model.routines, key = { it.id }) { r ->
            RoutineRow(r, Modifier.selectable(r.id == selected) { select(r.id) })
        }
        if (model.routineProblems.isNotEmpty()) {
            item { Text("Cannot read", Modifier.padding(16.dp), style = MaterialTheme.typography.titleSmall) }
            items(model.routineProblems, key = { "problem-" + it.workspace }) { p ->
                Column(Modifier.padding(horizontal = 16.dp, vertical = 6.dp)) {
                    Text(p.workspace)
                    Text(p.reason, color = Theme.failure, style = MaterialTheme.typography.labelMedium)
                }
            }
        }
    }
}

@Composable
private fun LinkBanner(model: NextModel) {
    val link = model.link as? NextModel.Link.Reconnecting ?: return
    Text(
        "Reconnecting… ${link.why}",
        Modifier.fillMaxWidth().background(MaterialTheme.colorScheme.surfaceVariant).padding(6.dp),
        style = MaterialTheme.typography.labelMedium,
    )
}

@Composable
fun Empty(text: String) {
    Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
        Text(text, style = MaterialTheme.typography.titleMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
    }
}

/** A row the user picks; the picked one is tinted. */
@Composable
fun Modifier.selectable(selected: Boolean, onClick: () -> Unit): Modifier =
    this
        .background(if (selected) MaterialTheme.colorScheme.secondaryContainer else Color.Transparent)
        .semantics { this.selected = selected }
        .clickable(onClick = onClick)

private fun nowSeconds() = System.currentTimeMillis() / 1000

private const val CLOCK_MS = 30_000L
private const val LIST_WIDTH = 380
