package tech.xdge.muxa.app.next

import androidx.activity.compose.BackHandler
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
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.VerticalDivider
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.isCtrlPressed
import androidx.compose.ui.input.key.isShiftPressed
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import tech.xdge.muxa.app.ui.Icons
import tech.xdge.muxa.app.ui.LocalPhoneLayout
import tech.xdge.muxa.app.ui.Theme
import tech.xdge.muxa.v2.LoopInfo
import tech.xdge.muxa.v2.Terminal
import tech.xdge.muxa.v2.V2Request

/**
 * Focus (ADR 0066 decision 1): one Task's terminal, with Needs you and the
 * Tasks on the left. The Inbox is a panel, closed by default.
 */
@Composable
fun Focus(model: NextModel, selected: String?, select: (String?) -> Unit, showHome: () -> Unit, newTask: () -> Unit) {
    // The panel beside the terminal shows the Inbox or the Reader, never both.
    var panel by rememberSaveable { mutableStateOf(Panel.NONE) }
    fun toggle(p: Panel) {
        panel = if (panel == p) Panel.NONE else p
    }
    var loopOpen by rememberSaveable { mutableStateOf(false) }
    val phone = LocalPhoneLayout.current
    val terminal = model.terminals.firstOrNull { it.id == selected }

    val list: @Composable (Modifier) -> Unit = { modifier ->
        Column(modifier) {
            Row(Modifier.fillMaxWidth().padding(horizontal = 8.dp), verticalAlignment = Alignment.CenterVertically) {
                TextButton(showHome, Modifier.testTag("mode.home")) {
                    Icon(Icons.list, null)
                    Text(" Home")
                }
                Box(Modifier.weight(1f))
                IconButton(newTask, Modifier.testTag("focus.newtask")) { Icon(Icons.newTask, "New Task") }
            }
            TerminalList(model, selected, select)
        }
    }
    val detail: @Composable (Modifier) -> Unit = { modifier ->
        // Ctrl+Shift+R opens the Reader and Ctrl+Shift+C Chat, as ⌘⇧R and ⌘⇧C do on iOS and the Mac.
        val readerKey = Modifier.onPreviewKeyEvent { e ->
            if (e.type != KeyEventType.KeyDown || !e.isCtrlPressed || !e.isShiftPressed) return@onPreviewKeyEvent false
            when (e.key) {
                Key.R -> true.also { if (terminal?.taskDir != null) toggle(Panel.READER) }
                Key.C -> true.also { if (terminal?.isTaskAgent == true) toggle(Panel.CHAT) }
                else -> false
            }
        }
        Column(modifier.then(readerKey)) {
            if (terminal == null) {
                Empty("Select a Task")
                return@Column
            }
            Row(Modifier.fillMaxWidth().padding(horizontal = 8.dp), verticalAlignment = Alignment.CenterVertically) {
                if (phone) IconButton({ select(null) }, Modifier.testTag("focus.back")) { Icon(Icons.back, "Back") }
                Text(terminal.title, Modifier.weight(1f), style = MaterialTheme.typography.titleMedium, maxLines = 1)
                val lp = model.loops[terminal.id]
                TextButton({ loopOpen = true }, Modifier.testTag("focus.loop"), enabled = terminal.isTaskAgent) {
                    Text(if (lp?.isActive == true) "Loop ${lp.attempts}/${lp.maxAttempts}" else "Loop")
                }
                TextButton({ toggle(Panel.CHAT) }, Modifier.testTag("focus.chat"), enabled = terminal.isTaskAgent) {
                    Text("Chat")
                }
                TextButton({ toggle(Panel.READER) }, Modifier.testTag("focus.reader"), enabled = terminal.taskDir != null) {
                    Text("Reader")
                }
                TextButton({ toggle(Panel.INBOX) }, Modifier.testTag("focus.inbox")) { Text("Inbox") }
            }
            val shownPanel: @Composable (Modifier) -> Unit = { m ->
                when (panel) {
                    Panel.INBOX -> InboxPanel(model, m)
                    Panel.READER -> key(terminal.id) { ReaderPanel(model, terminal, m) }
                    Panel.CHAT -> key(terminal.id) { ChatPanel(model, terminal, m) }
                    Panel.NONE -> {}
                }
            }
            Row(Modifier.weight(1f)) {
                key(terminal.id) { TerminalPane(model, terminal.id, Modifier.weight(1f)) }
                if (panel != Panel.NONE && !phone) {
                    VerticalDivider()
                    val width = if (panel == Panel.INBOX) PANEL_WIDTH else READER_WIDTH
                    shownPanel(Modifier.width(width.dp).fillMaxHeight())
                }
            }
            // On a phone the panel shares the screen with the terminal, below it.
            if (panel != Panel.NONE && phone) shownPanel(Modifier.fillMaxWidth().weight(1f))
            if (loopOpen) LoopDialog(model, terminal) { loopOpen = false }
        }
    }

    if (phone) {
        if (terminal == null) {
            list(Modifier.fillMaxSize())
        } else {
            BackHandler { select(null) }
            detail(Modifier.fillMaxSize())
        }
    } else {
        Row(Modifier.fillMaxSize()) {
            list(Modifier.width(LIST_WIDTH.dp).fillMaxHeight())
            VerticalDivider()
            detail(Modifier.weight(1f).fillMaxHeight())
        }
    }
}

@Composable
private fun TerminalList(model: NextModel, selected: String?, select: (String) -> Unit) {
    val waiting = model.needsYou
    val tasks = model.terminals.filter { it.live && it.isTaskAgent && it !in waiting }
    val others = model.terminals.filter { it.live && !it.isTaskAgent }
    LazyColumn(Modifier.fillMaxSize()) {
        fun section(title: String, list: List<Terminal>) {
            if (list.isEmpty()) return
            item(key = "section-$title") { Text(title, Modifier.padding(16.dp, 12.dp, 16.dp, 4.dp), style = MaterialTheme.typography.titleSmall) }
            items(list, key = { it.id }) { t ->
                TerminalRow(t, model.loops[t.id], Modifier.selectable(t.id == selected) { select(t.id) })
            }
        }
        section("Needs you", waiting)
        section("Tasks", tasks)
        section("Terminals", others)
    }
}

@Composable
private fun TerminalRow(t: Terminal, loop: LoopInfo?, modifier: Modifier) {
    Column(modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 8.dp).testTag("terminal.${t.id}")) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text(t.title, Modifier.weight(1f))
            t.status?.state?.let { state ->
                Text(
                    state, style = MaterialTheme.typography.labelMedium,
                    color = if (state == "blocked") Theme.attention else MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
        }
        t.taskDir?.let {
            Text(it, style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant,
                maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
        loop?.let {
            Text(
                LoopText.badge(it), style = MaterialTheme.typography.labelSmall,
                color = if (it.reason == "limit") Theme.attention else MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
}

/** The Inbox beside a terminal: Capture and the open items; Done and Later by swipe. */
@Composable
private fun InboxPanel(model: NextModel, modifier: Modifier) {
    val now = System.currentTimeMillis() / 1000
    Column(modifier.testTag("focus.inboxpanel")) {
        Column(Modifier.padding(8.dp)) { CaptureField(model) }
        LazyColumn {
            items(model.inbox.items.filter { !it.isLater(now) }, key = { it.id }) { item ->
                Row(verticalAlignment = Alignment.CenterVertically) {
                    ItemRow(item, now, Modifier.weight(1f))
                    IconButton({ model.inbox(V2Request.InboxDone(item.id)) }) { Icon(Icons.check, "Done") }
                }
            }
        }
    }
}

private enum class Panel { NONE, INBOX, READER, CHAT }

private const val LIST_WIDTH = 300
private const val PANEL_WIDTH = 320
private const val READER_WIDTH = 440
