package tech.xdge.muxa.app.next

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.MutableState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import tech.xdge.muxa.app.ui.Icons
import tech.xdge.muxa.app.ui.Palette
import tech.xdge.muxa.app.ui.StateColors
import tech.xdge.muxa.app.ui.Theme
import tech.xdge.muxa.v2.LoopInfo
import tech.xdge.muxa.v2.Terminal

/**
 * A terminal's state as its program reports it (OSC 7501). [UNKNOWN] when it
 * reports none: muxa never guesses one (invariant 6). [PAUSED] is muxa's own:
 * the Task has no agent (ADR 0104).
 */
enum class TaskState(val title: String, val color: Color) {
    NEEDS_YOU("Needs you", StateColors.needsYou),
    WORKING("Working", StateColors.working),
    DONE("Done", StateColors.done),
    IDLE("Idle", StateColors.idle),
    UNKNOWN("No state", Palette.borderStrong),
    PAUSED("Paused", Palette.textTertiary),
}

val Terminal.state: TaskState
    get() = when (status?.state) {
        null -> TaskState.UNKNOWN
        in NextModel.WAITING -> TaskState.NEEDS_YOU
        "working" -> TaskState.WORKING
        "done" -> TaskState.DONE
        else -> TaskState.IDLE
    }

/** The Workspace a Task belongs to; null for terminals muxa did not start as a Task. */
val Terminal.workspace: String? get() = labels["muxa.workspace"]

/** Where a terminal works: its repo and kind of place, or its folder. */
val Terminal.place: String?
    get() {
        val repo = labels["muxa.repo"]?.substringAfterLast('/')
        return when {
            repo != null && branch != null -> "$repo · branch"
            else -> taskDir
        }
    }

/** The path over a Task's title in Focus, as the design's "xdge · muxa branch". */
fun Terminal.path(): String = listOfNotNull(workspace, place, status?.app).joinToString(" · ").ifEmpty { "Terminal" }

/**
 * The phone's Workspaces tab: each Workspace's Tasks in a card, with New Task,
 * then the other live terminals in a card that [othersOpen] opens.
 */
@Composable
fun WorkspaceList(model: NextModel, othersOpen: MutableState<Boolean>, select: (String) -> Unit, newTask: (TaskSeed) -> Unit) {
    val groups = TerminalGroups.of(model.terminals, model.paused)
    val byWorkspace = groups.byWorkspace
    LazyColumn(
        Modifier.fillMaxSize(),
        contentPadding = PaddingValues(horizontal = 12.dp, vertical = 4.dp),
        verticalArrangement = Arrangement.spacedBy(14.dp),
    ) {
        items(byWorkspace.keys.toList(), key = { "ws-$it" }) { ws ->
            val tasks = byWorkspace.getValue(ws)
            Card(ws ?: "Tasks", "${tasks.size} ${if (tasks.size == 1) "Task" else "Tasks"}") {
                for (t in tasks) WorkspaceRow(t, model.loops[t.id], groups.stateOf(t), model.rowMenu(t, groups) {}) { select(t.id) }
                CardRow(Modifier.testTag("workspace.newtask.${ws.orEmpty()}"), { newTask(TaskSeed(workspace = ws)) }) {
                    Text("+ New Task", color = Palette.accent, fontSize = 14.sp)
                }
            }
        }
        if (groups.others.isNotEmpty()) {
            item(key = "terminals") {
                var open by othersOpen
                Card(OTHERS, "${groups.others.size}", open = open, toggle = { open = !open }) {
                    if (open) for (t in groups.others) WorkspaceRow(t, null, t.state, null) { select(t.id) }
                }
            }
        }
        if (groups.isEmpty) {
            item { Text("No Tasks yet. Start one with +.", Modifier.padding(8.dp), color = Palette.textSecondary) }
        }
    }
}

/** A card of rows. With [toggle], its header opens and closes it. */
@Composable
private fun Card(title: String, detail: String, open: Boolean = true, toggle: (() -> Unit)? = null, rows: @Composable () -> Unit) {
    Column(
        Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(14.dp))
            .background(Palette.raised)
            .border(1.dp, Palette.border, RoundedCornerShape(14.dp)),
    ) {
        val header = if (toggle == null) Modifier else Modifier.toggler(open, toggle)
        Row(
            header.fillMaxWidth().padding(start = 14.dp, end = 14.dp, top = 12.dp, bottom = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(6.dp),
        ) {
            Text(title, Modifier.weight(1f), fontSize = 16.sp, fontWeight = FontWeight.SemiBold)
            Text(detail, fontSize = 12.sp, color = Palette.textTertiary)
            if (toggle != null) Chevron(open)
        }
        rows()
    }
}

@Composable
private fun CardRow(modifier: Modifier, onClick: () -> Unit, onLongClick: (() -> Unit)? = null, content: @Composable RowScope.() -> Unit) {
    HorizontalDivider(color = Palette.divider)
    Row(
        modifier.fillMaxWidth().heightIn(min = 48.dp).pressable(onClick, onLongClick, Role.Button).padding(horizontal = 14.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) { content() }
}

@Composable
private fun WorkspaceRow(t: Terminal, loop: LoopInfo?, state: TaskState, menu: RowMenu?, onClick: () -> Unit) {
    var menuOpen by remember { mutableStateOf(false) }
    Box {
        CardRow(Modifier.testTag("terminal.${t.id}").pausedState(state), onClick, menu?.let { { menuOpen = true } }) {
            Dot(state.color)
            Column(
                Modifier.weight(1f).padding(vertical = 6.dp).alpha(state.alpha),
                verticalArrangement = Arrangement.spacedBy(1.dp),
            ) {
                Text(t.title, maxLines = 1, overflow = TextOverflow.Ellipsis)
                t.bot?.let { TalkChip(it, Modifier.testTag("terminal.${t.id}.bot")) }
                t.place?.let { Text(it, fontSize = 12.sp, color = Palette.textTertiary, maxLines = 1, overflow = TextOverflow.Ellipsis) }
                loop?.let { LoopBadge(it) }
            }
            if (state != TaskState.UNKNOWN) {
                Text(state.title.lowercase(), Modifier.testTag("terminal.${t.id}.state"), fontSize = 12.5.sp, color = state.color)
            }
            Icon(Icons.chevron, null, Modifier.size(16.dp), tint = Palette.textTertiary)
        }
        menu?.let { TaskRowMenu(it, menuOpen) { menuOpen = false } }
    }
}

/** The tablet's Tasks: grouped by state, Needs you first and Paused last, then the other live terminals, which [othersOpen] opens. */
@Composable
fun StateList(model: NextModel, othersOpen: MutableState<Boolean>, selected: String?, select: (String) -> Unit) {
    val groups = TerminalGroups.of(model.terminals, model.paused)
    val byState = TaskState.entries.associateWith { s -> groups.tasks.filter { groups.stateOf(it) == s } }.filterValues { it.isNotEmpty() }
    var open by othersOpen
    LazyColumn(
        Modifier.fillMaxSize(),
        contentPadding = PaddingValues(start = 12.dp, end = 12.dp, bottom = 16.dp),
        verticalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        for ((state, list) in byState) {
            item(key = "state-$state") {
                SectionHeader(state.title, if (state == TaskState.UNKNOWN) Palette.textTertiary else state.color, list.size)
            }
            items(list, key = { it.id }) { t ->
                // A Task resumed while it is shown stays shown under its new agent.
                val menu = model.rowMenu(t, groups) { if (t.id == selected) select(it.id) }
                StateRow(t, model.loops[t.id], state, menu, t.id == selected) { select(t.id) }
            }
        }
        if (groups.others.isNotEmpty()) {
            item(key = "others") {
                Row(
                    Modifier.fillMaxWidth().clip(RoundedCornerShape(8.dp)).toggler(open) { open = !open }.padding(end = 6.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Box(Modifier.weight(1f)) { SectionHeader(OTHERS, Palette.textTertiary, groups.others.size) }
                    Chevron(open)
                }
            }
            if (open) items(groups.others, key = { it.id }) { t -> StateRow(t, null, t.state, null, t.id == selected) { select(t.id) } }
        }
        if (groups.isEmpty) item { Text("No Tasks yet. Start one with +.", Modifier.padding(8.dp), color = Palette.textSecondary) }
    }
}

/** The header of the Other terminals section: it opens and closes the section. */
private fun Modifier.toggler(open: Boolean, toggle: () -> Unit): Modifier =
    clickable(role = Role.Button, onClick = toggle)
        .semantics { stateDescription = if (open) "Expanded" else "Collapsed" }
        .testTag(OTHERS_TAG)

@Composable
private fun Chevron(open: Boolean) {
    Icon(Icons.chevron, null, Modifier.size(16.dp).rotate(if (open) 90f else 0f), tint = Palette.textTertiary)
}

@Composable
private fun StateRow(t: Terminal, loop: LoopInfo?, state: TaskState, menu: RowMenu?, picked: Boolean, onClick: () -> Unit) {
    var menuOpen by remember { mutableStateOf(false) }
    Box {
        Row(
            Modifier
                .fillMaxWidth()
                .heightIn(min = 48.dp)
                .clip(RoundedCornerShape(10.dp))
                .background(if (picked) Palette.picked else Color.Transparent)
                .semantics { selected = picked }
                .pausedState(state)
                .pressable(onClick, menu?.let { { menuOpen = true } })
                .padding(10.dp)
                .testTag("terminal.${t.id}"),
            horizontalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            Dot(state.color, modifier = Modifier.padding(top = 5.dp))
            Column(Modifier.weight(1f).alpha(state.alpha), verticalArrangement = Arrangement.spacedBy(3.dp)) {
                Text(t.path(), fontSize = 12.sp, color = Palette.textSecondary, maxLines = 1, overflow = TextOverflow.Ellipsis)
                Text(t.title, fontSize = 14.sp, lineHeight = 19.sp, color = Palette.text)
                t.bot?.let { TalkChip(it, Modifier.testTag("terminal.${t.id}.bot")) }
                loop?.let { LoopBadge(it) }
            }
        }
        menu?.let { TaskRowMenu(it, menuOpen) { menuOpen = false } }
    }
}

/** A click, and a long press when [onLongClick] is set: a Task row's menu. */
private fun Modifier.pressable(onClick: () -> Unit, onLongClick: (() -> Unit)?, role: Role? = null): Modifier =
    if (onLongClick == null) {
        clickable(role = role, onClick = onClick)
    } else {
        combinedClickable(role = role, onLongClickLabel = "Task actions", onLongClick = onLongClick, onClick = onClick)
    }

/** Says Paused to a screen reader, which does not see the dimming. */
private fun Modifier.pausedState(state: TaskState): Modifier =
    if (state == TaskState.PAUSED) semantics { stateDescription = TaskState.PAUSED.title } else this

/** A paused Task's row is dimmed: nothing runs in it. */
private val TaskState.alpha: Float get() = if (this == TaskState.PAUSED) PAUSED_ALPHA else 1f

private const val PAUSED_ALPHA = 0.55f

@Composable
private fun LoopBadge(loop: LoopInfo) {
    Text(
        LoopText.badge(loop), fontSize = 11.sp,
        color = if (loop.reason == "limit") Theme.attention else Palette.textTertiary,
    )
}

private const val OTHERS = "Other terminals"
private const val OTHERS_TAG = "terminals.other"
