package tech.xdge.muxa.app.next

import android.provider.OpenableColumns
import androidx.activity.compose.BackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.background
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
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.isCtrlPressed
import androidx.compose.ui.input.key.isShiftPressed
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import tech.xdge.muxa.app.ui.Corners
import tech.xdge.muxa.app.ui.Icons
import tech.xdge.muxa.app.ui.LocalPhoneLayout
import tech.xdge.muxa.app.ui.Palette
import tech.xdge.muxa.v2.Terminal
import tech.xdge.muxa.v2.V2Request

/** What Focus shows of a Task. The terminal stays attached under Chat and the Reader. */
private enum class View { CHAT, TERMINAL, READER }

/**
 * Focus (ADR 0066 decision 1): one Task as Chat, its terminal or the Reader,
 * with the Tasks beside it on a tablet. The Inbox is a panel, closed by default.
 */
@Composable
fun Focus(model: NextModel, nav: Nav, selected: String?, select: (String?) -> Unit) {
    val phone = LocalPhoneLayout.current
    val terminal = model.terminals.firstOrNull { it.id == selected }
    // Here, not in the list: the phone's list leaves the screen while a terminal is open.
    val othersOpen = rememberSaveable { mutableStateOf(false) }
    if (phone) {
        if (terminal == null) {
            PhoneTabs(nav) {
                LargeTitle("Workspaces") {
                    BoxedIconButton(Icons.add, "New Task", { nav.newTask(TaskSeed()) }, Modifier.testTag("home.newtask"), round = true)
                }
                Box(Modifier.weight(1f)) { WorkspaceList(model, othersOpen, select, nav.newTask) }
            }
        } else {
            BackHandler { select(null) }
            key(terminal.id) { TaskView(model, terminal, Modifier.fillMaxSize()) { select(null) } }
        }
    } else {
        Row(Modifier.fillMaxSize()) {
            Sidebar(model, nav) { StateList(model, othersOpen, selected, select) }
            if (terminal == null) {
                Box(Modifier.weight(1f).fillMaxHeight()) { Empty("Select a Task") }
            } else {
                key(terminal.id) { TaskView(model, terminal, Modifier.weight(1f).fillMaxHeight(), null) }
            }
        }
    }
}

/** One Task: its header, the view, and the line to answer it. [back] is the phone's way back. */
@Composable
private fun TaskView(model: NextModel, terminal: Terminal, modifier: Modifier, back: (() -> Unit)?) {
    val phone = back != null
    var view by rememberSaveable { mutableStateOf(View.TERMINAL) }
    var inbox by rememberSaveable { mutableStateOf(false) }
    var loopOpen by rememberSaveable { mutableStateOf(false) }
    // A Task's shells (ADR 0079): the picked one while it lives, else the agent.
    var picked by rememberSaveable(terminal.id) { mutableStateOf<String?>(null) }
    val shells = model.terminals.shellsOf(terminal)
    val pane = shells.firstOrNull { it.id == picked }?.id ?: terminal.id
    val chat = rememberChat(model, terminal)
    val canRead = terminal.taskDir != null
    val shown = when {
        view == View.CHAT && chat == null -> View.TERMINAL
        view == View.READER && !canRead -> View.TERMINAL
        else -> view
    }
    fun toggle(v: View) {
        view = if (shown == v) View.TERMINAL else v
    }
    // Ctrl+Shift+R opens the Reader and Ctrl+Shift+C Chat, as ⌘⇧R and ⌘⇧C do on iOS and the Mac.
    val viewKeys = Modifier.onPreviewKeyEvent { e ->
        if (e.type != KeyEventType.KeyDown || !e.isCtrlPressed || !e.isShiftPressed) return@onPreviewKeyEvent false
        when (e.key) {
            Key.R -> true.also { if (canRead) toggle(View.READER) }
            Key.C -> true.also { if (chat != null) toggle(View.CHAT) }
            else -> false
        }
    }
    val order = if (phone) listOf(View.CHAT, View.TERMINAL, View.READER) else listOf(View.CHAT, View.READER, View.TERMINAL)
    val segments = order.map {
        when (it) {
            View.CHAT -> Segment("Chat", "focus.chat", chat != null)
            View.TERMINAL -> Segment("Terminal", "focus.terminal")
            View.READER -> Segment("Reader", "focus.reader", canRead)
        }
    }
    val switcher: @Composable (Modifier) -> Unit = { m -> Segmented(segments, order.indexOf(shown), { view = order[it] }, m) }
    val prompt = chat?.prompt
    val needsYou: @Composable (Modifier) -> Unit = { m -> prompt?.let { p -> NeedsYouBar(p, { model.send(it, terminal.id) }, m) } }

    Column(modifier.then(viewKeys)) {
        Header(model, terminal, back, { loopOpen = true }, inbox, { inbox = !inbox }, if (phone) null else switcher)
        if (phone) {
            switcher(Modifier.fillMaxWidth().padding(horizontal = 12.dp).padding(bottom = 10.dp))
            needsYou(Modifier.padding(start = 12.dp, end = 12.dp, bottom = 10.dp))
        }
        if (shells.isNotEmpty()) ShellPicker(terminal, shells, pane) { picked = it }
        Row(Modifier.weight(1f)) {
            val inset = if (phone) Modifier.padding(horizontal = 6.dp).clip(RoundedCornerShape(10.dp)) else Modifier
            Box(Modifier.weight(1f).fillMaxHeight().then(inset)) {
                key(pane) { TerminalPane(model, pane, Modifier.fillMaxSize()) }
                // Chat and the Reader cover the terminal, which stays attached at its size so answers reach it.
                when (shown) {
                    View.CHAT -> chat?.let { ChatView(it, Cover) }
                    View.READER -> ReaderPanel(model, terminal, Cover)
                    View.TERMINAL -> {}
                }
                if (inbox && phone) InboxPanel(model, Cover)
            }
            if (inbox && !phone) InboxPanel(model, Modifier.width(PANEL_WIDTH.dp).fillMaxHeight().background(Palette.surface))
        }
        Column(
            Modifier.fillMaxWidth().then(if (phone) Modifier else Modifier.background(Palette.surface).topBorder())
                .padding(horizontal = if (phone) 10.dp else 24.dp, vertical = 10.dp),
            verticalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            if (!phone) needsYou(Modifier)
            Composer(model, terminal, round = phone)
        }
    }
    if (loopOpen) LoopDialog(model, terminal) { loopOpen = false }
}

/** Agent, Shell 1, Shell 2…: the Task's terminals. Another device's new shell shows here too. */
@Composable
private fun ShellPicker(agent: Terminal, shells: List<Terminal>, shown: String, pick: (String?) -> Unit) {
    Row(
        Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 6.dp).testTag("task.terminals"),
        horizontalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        Chip("Agent", agent.id == shown, "task.terminal.${agent.id}") { pick(null) }
        shells.forEachIndexed { n, shell ->
            Chip("Shell ${n + 1}", shell.id == shown, "task.terminal.${shell.id}") { pick(shell.id) }
        }
    }
}

@Composable
private fun Chip(title: String, on: Boolean, tag: String, onClick: () -> Unit) {
    TextButton(
        onClick,
        Modifier.testTag(tag).semantics { selected = on }
            .background(if (on) Palette.picked else Palette.control, RoundedCornerShape(50)),
    ) {
        Text(title, fontSize = 13.sp, fontWeight = if (on) FontWeight.SemiBold else FontWeight.Normal,
            color = if (on) Palette.text else Palette.textSecondary)
    }
}

/** Sends files from this device into the Task's working place (ADR 0081); each path is then typed into the agent. */
@Composable
private fun UploadButton(model: NextModel, terminal: Terminal) {
    val context = LocalContext.current
    val pick = rememberLauncherForActivityResult(ActivityResultContracts.OpenMultipleDocuments()) { uris ->
        for (uri in uris) {
            val name = context.contentResolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)
                ?.use { c -> if (c.moveToFirst()) c.getString(0) else null } ?: "upload"
            val bytes = runCatching { context.contentResolver.openInputStream(uri)?.use { it.readBytes() } }.getOrNull()
            if (bytes == null) model.requestError = "$name could not be read." else model.upload(bytes, name, terminal.id)
        }
    }
    IconButton({ pick.launch(arrayOf("*/*")) }, Modifier.testTag("focus.upload"), enabled = terminal.isTaskAgent) {
        Icon(Icons.upload, "Upload")
    }
}

/** Covers the terminal, and keeps touches from reaching it. */
private val Cover = Modifier.fillMaxSize().background(Palette.ground).pointerInput(Unit) {}

@Composable
private fun Header(
    model: NextModel,
    terminal: Terminal,
    back: (() -> Unit)?,
    openLoop: () -> Unit,
    inbox: Boolean,
    toggleInbox: () -> Unit,
    switcher: (@Composable (Modifier) -> Unit)?,
) {
    val phone = back != null
    Row(
        Modifier.fillMaxWidth().then(if (phone) Modifier else Modifier.bottomBorder())
            .padding(start = if (phone) 4.dp else 24.dp, end = if (phone) 4.dp else 16.dp, top = 8.dp, bottom = if (phone) 8.dp else 12.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        back?.let { IconButton(it, Modifier.testTag("focus.back")) { Icon(Icons.backChevron, "Back", tint = Palette.accent) } }
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(if (phone) 0.dp else 4.dp)) {
            Text(terminal.path(), fontSize = if (phone) 12.sp else 13.sp, color = Palette.textSecondary, maxLines = 1, overflow = TextOverflow.Ellipsis)
            Text(terminal.title, fontSize = if (phone) 16.sp else 19.sp, fontWeight = FontWeight.SemiBold, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
        terminal.bot?.let { TalkChip(it, Modifier.padding(horizontal = 6.dp).testTag("focus.bot")) }
        val lp = model.loops[terminal.id]
        if (lp?.isActive == true) {
            TextButton(openLoop, Modifier.testTag("focus.loop"), shape = Corners.link) {
                Icon(Icons.loop, null, Modifier.padding(end = 4.dp))
                Text("Loop ${lp.attempts}/${lp.maxAttempts}")
            }
        } else {
            IconButton(openLoop, Modifier.testTag("focus.loop"), enabled = terminal.isTaskAgent) { Icon(Icons.loop, "Loop") }
        }
        UploadButton(model, terminal)
        IconButton(toggleInbox, Modifier.testTag("focus.inbox")) {
            Icon(Icons.inbox, if (inbox) "Close the Inbox" else "Inbox", tint = if (inbox) Palette.accent else Palette.textSecondary)
        }
        switcher?.invoke(Modifier.padding(start = 8.dp).width(SWITCHER_WIDTH.dp))
    }
}

/** The Inbox beside a terminal: Capture and the open items; Done by its button. */
@Composable
private fun InboxPanel(model: NextModel, modifier: Modifier) {
    val now = System.currentTimeMillis() / 1000
    Column(modifier.testTag("focus.inboxpanel")) {
        Column(Modifier.padding(12.dp)) { CaptureField(model) }
        LazyColumn(Modifier.padding(horizontal = 12.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
            items(model.inbox.items.filter { !it.isLater(now) }, key = { it.id }) { item ->
                Row(verticalAlignment = Alignment.CenterVertically) {
                    ItemRow(item, now, Modifier.weight(1f))
                    IconButton({ model.inbox(V2Request.InboxDone(item.id)) }) { Icon(Icons.check, "Done", tint = Palette.textSecondary) }
                }
            }
        }
    }
}

private const val PANEL_WIDTH = 340
private const val SWITCHER_WIDTH = 300
