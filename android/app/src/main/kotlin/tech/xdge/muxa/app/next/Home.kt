package tech.xdge.muxa.app.next

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.ui.draw.clip
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
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
import tech.xdge.muxa.app.ui.Icons
import tech.xdge.muxa.app.ui.LocalPhoneLayout
import tech.xdge.muxa.app.ui.Palette
import tech.xdge.muxa.app.ui.Theme
import tech.xdge.muxa.v2.InboxItem
import tech.xdge.muxa.v2.ItemKind
import tech.xdge.muxa.v2.Terminal
import tech.xdge.muxa.v2.V2Request

/**
 * Home (ADR 0066): the Inbox and the Routines, a list and a detail. A phone
 * shows them as two screens; a tablet shows the list in the sidebar.
 */
@Composable
fun Home(model: NextModel, nav: Nav, openTerminal: (Terminal) -> Unit) {
    val inboxPart = nav.section != Section.ROUTINES
    val newTask = nav.newTask
    var selectedItem by rememberSaveable { mutableStateOf<String?>(null) }
    var selectedRoutine by rememberSaveable { mutableStateOf<String?>(null) }
    var filter by remember { mutableStateOf(InboxFilter()) }
    var reordering by remember { mutableStateOf(false) }
    var now by remember { mutableLongStateOf(nowSeconds()) }
    val capture = remember { FocusRequester() }
    val phone = LocalPhoneLayout.current
    val scope = rememberCoroutineScope()
    val actions = remember(model) { ItemActionState(model, scope) }

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
        val item = selected?.takeIf { inboxPart && it.id in model.inbox.items.map(InboxItem::id) }
        // While the comment field is open, its own keys win.
        val commenting = item != null && actions.commenting == item.id
        when {
            e.isCtrlPressed && e.key == Key.N -> capture.requestFocus().let { true }
            e.isAltPressed && e.key == Key.DirectionUp && item != null -> step(-1).let { true }
            e.isAltPressed && e.key == Key.DirectionDown && item != null -> step(1).let { true }
            e.isCtrlPressed && e.key == Key.Enter && item != null && !commenting -> true.also {
                actions.main(item) { selectedItem = nextVisible(item.id) }
            }
            e.isCtrlPressed && e.key == Key.R && item?.kind == ItemKind.REVIEW -> true.also { actions.commenting = item.id }
            e.isCtrlPressed && e.key == Key.R && item?.kind == ItemKind.FAILED -> true.also {
                actions.retry(item.id) { selectedItem = nextVisible(item.id) }
            }
            e.isCtrlPressed && e.key == Key.Backspace && item != null -> act(V2Request.InboxDone(item.id)).let { true }
            e.isCtrlPressed && e.key == Key.L && item != null -> act(V2Request.InboxLater(item.id, LaterChoice.TOMORROW.until())).let { true }
            else -> false
        }
    }

    val tools: @Composable RowScope.() -> Unit = {
        if (inboxPart) InboxTools(model, filter, { filter = it }, reordering) { reordering = it }
    }
    val list: @Composable ColumnScope.() -> Unit = {
        if (inboxPart) {
            Column(Modifier.padding(horizontal = if (phone) 18.dp else 12.dp, vertical = 6.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                NeedsYouBanner(model, openTerminal) { nav.go(Section.TASKS) }
                CaptureField(model, capture)
            }
            Box(Modifier.weight(1f)) { InboxList(model, visible, filter, now, selectedItem, reordering) { selectedItem = it } }
        } else {
            Box(Modifier.weight(1f)) { RoutineList(model, selectedRoutine) { selectedRoutine = it } }
        }
        LinkBanner(model)
    }
    val detail: @Composable (Modifier) -> Unit = { modifier ->
        Box(modifier) {
            if (inboxPart) {
                if (selected != null) {
                    val position = model.inbox.items.indexOfFirst { it.id == selected.id }.takeIf { it >= 0 }?.plus(1)
                    InboxItemPane(model, selected, position, actions, openTerminal, newTask) { selectedItem = nextVisible(selected.id) }
                } else {
                    Empty(if (model.inbox.items.isEmpty()) "Inbox zero" else "Select an item")
                }
            } else {
                model.routines.firstOrNull { it.id == selectedRoutine }?.let { RoutinePane(model, it) } ?: Empty("Select a Routine")
            }
        }
    }

    val shown = if (inboxPart) selectedItem else selectedRoutine
    fun back() {
        if (inboxPart) selectedItem = null else selectedRoutine = null
    }
    Box(Modifier.fillMaxSize().then(keys)) {
        if (phone) {
            if (shown == null) {
                PhoneTabs(nav) {
                    LargeTitle(if (inboxPart) "Inbox" else "Routines") {
                        if (inboxPart) HostMenu(model) { nav.go(Section.SETTINGS) }
                        tools()
                    }
                    list()
                }
            } else {
                BackHandler { back() }
                Column(Modifier.fillMaxSize()) {
                    IconButton(::back, Modifier.padding(start = 4.dp).testTag("detail.back")) {
                        Icon(Icons.backChevron, "Back", tint = Palette.accent)
                    }
                    detail(Modifier.fillMaxSize())
                }
            }
        } else {
            Row(Modifier.fillMaxSize()) {
                Sidebar(model, nav, actions = tools, content = list)
                detail(Modifier.weight(1f).fillMaxHeight())
            }
        }
    }
}

@Composable
private fun RoutineList(model: NextModel, selected: String?, select: (String) -> Unit) {
    LaunchedEffect(Unit) { model.refreshRoutines() }
    LazyColumn(
        Modifier.fillMaxSize(),
        contentPadding = PaddingValues(horizontal = 12.dp, vertical = 6.dp),
        verticalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        val groups = routineGroups(model.routines)
        for (group in groups) {
            // Headers only once some Routine has a Bot.
            if (groups.size > 1 || group.bot != null) {
                item(key = "group-" + (group.bot ?: "")) {
                    Text(
                        group.bot ?: "Other",
                        Modifier.padding(start = 6.dp, top = 10.dp, bottom = 2.dp).testTag("routines.group.${group.bot ?: "other"}"),
                        style = MaterialTheme.typography.titleSmall,
                        color = Palette.textSecondary,
                    )
                }
            }
            items(group.routines, key = { it.id }) { r ->
                RoutineRow(r, Modifier.clip(RoundedCornerShape(14.dp)).background(Palette.raised).selectable(r.id == selected) { select(r.id) })
            }
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
        .background(if (selected) Palette.picked else Color.Transparent)
        .semantics { this.selected = selected }
        .clickable(onClick = onClick)

private fun nowSeconds() = System.currentTimeMillis() / 1000

private const val CLOCK_MS = 30_000L
