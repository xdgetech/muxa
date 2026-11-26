package tech.xdge.muxa.app.next

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
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
import tech.xdge.muxa.app.ui.Icons
import tech.xdge.muxa.app.ui.LocalPhoneLayout
import tech.xdge.muxa.app.ui.Palette
import tech.xdge.muxa.v2.InboxItem
import tech.xdge.muxa.v2.ItemKind
import tech.xdge.muxa.v2.Terminal
import tech.xdge.muxa.v2.V2Request

/**
 * Home (ADR 0066): the Inbox, a list and a detail. A phone
 * shows them as two screens; a tablet shows the list in the sidebar.
 */
@Composable
fun Home(model: NextModel, nav: Nav, openTerminal: (Terminal) -> Unit) {
    val newTask = nav.newTask
    var selectedItem by rememberSaveable { mutableStateOf<String?>(null) }
    var filter by remember { mutableStateOf(InboxFilter()) }
    var reordering by remember { mutableStateOf(false) }
    val capture = remember { FocusRequester() }
    val phone = LocalPhoneLayout.current
    val scope = rememberCoroutineScope()
    val actions = remember(model) { ItemActionState(model, scope) }

    val visible = model.inbox.items.filter(filter::shows)
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
        val item = selected?.takeIf { it.id in model.inbox.items.map(InboxItem::id) }
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
            e.isCtrlPressed && e.key == Key.L && item != null -> act(V2Request.InboxLater(item.id)).let { true }
            else -> false
        }
    }

    val tools: @Composable RowScope.() -> Unit = {
        InboxTools(model, filter, { filter = it }, reordering) { reordering = it }
    }
    val list: @Composable ColumnScope.() -> Unit = {
        Column(Modifier.padding(horizontal = if (phone) 18.dp else 12.dp, vertical = 6.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
            NeedsYouBanner(model, openTerminal) { nav.go(Section.TASKS) }
            CaptureField(model, capture)
        }
        Box(Modifier.weight(1f)) { InboxList(model, visible, filter, selectedItem, reordering) { selectedItem = it } }
        LinkBanner(model)
    }
    val detail: @Composable (Modifier) -> Unit = { modifier ->
        Box(modifier) {
            if (selected != null) {
                val position = model.inbox.items.indexOfFirst { it.id == selected.id }.takeIf { it >= 0 }?.plus(1)
                InboxItemPane(model, selected, position, actions, openTerminal, newTask) { selectedItem = nextVisible(selected.id) }
            } else {
                Empty(if (model.inbox.items.isEmpty()) "Inbox zero" else "Select an item")
            }
        }
    }

    val shown = selectedItem
    fun back() {
        selectedItem = null
    }
    Box(Modifier.fillMaxSize().then(keys)) {
        if (phone) {
            if (shown == null) {
                PhoneTabs(nav) {
                    LargeTitle("Inbox") {
                        HostMenu(model) { nav.go(Section.SETTINGS) }
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
