package tech.xdge.muxa.app.next

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
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
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
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
 * reports none: muxa never guesses one (invariant 6).
 */
enum class TaskState(val title: String, val color: Color) {
    NEEDS_YOU("Needs you", StateColors.needsYou),
    WORKING("Working", StateColors.working),
    DONE("Done", StateColors.done),
    IDLE("Idle", StateColors.idle),
    UNKNOWN("Terminals", Palette.borderStrong),
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

/** The phone's Workspaces tab: each Workspace's Tasks in a card, with New Task. */
@Composable
fun WorkspaceList(model: NextModel, select: (String) -> Unit, newTask: (TaskSeed) -> Unit) {
    val live = model.terminals.filter { it.live }
    val groups = live.filter { it.isTaskAgent }.groupBy { it.workspace }
    val others = live.filter { !it.isTaskAgent }
    LazyColumn(
        Modifier.fillMaxSize(),
        contentPadding = PaddingValues(horizontal = 12.dp, vertical = 4.dp),
        verticalArrangement = Arrangement.spacedBy(14.dp),
    ) {
        items(groups.keys.toList(), key = { "ws-$it" }) { ws ->
            val tasks = groups.getValue(ws)
            Card(ws ?: "Tasks", "${tasks.size} ${if (tasks.size == 1) "Task" else "Tasks"}") {
                for (t in tasks) WorkspaceRow(t, model.loops[t.id]) { select(t.id) }
                CardRow(Modifier.testTag("workspace.newtask.${ws.orEmpty()}"), { newTask(TaskSeed(workspace = ws)) }) {
                    Text("+ New Task", color = Palette.accent, fontSize = 14.sp)
                }
            }
        }
        if (others.isNotEmpty()) {
            item(key = "terminals") {
                Card("Terminals", "${others.size}") {
                    for (t in others) WorkspaceRow(t, null) { select(t.id) }
                }
            }
        }
        if (live.isEmpty()) {
            item { Text("No Tasks yet. Start one with +.", Modifier.padding(8.dp), color = Palette.textSecondary) }
        }
    }
}

@Composable
private fun Card(title: String, detail: String, rows: @Composable () -> Unit) {
    Column(
        Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(14.dp))
            .background(Palette.raised)
            .border(1.dp, Palette.border, RoundedCornerShape(14.dp)),
    ) {
        Row(Modifier.padding(start = 14.dp, end = 14.dp, top = 12.dp, bottom = 8.dp), verticalAlignment = Alignment.CenterVertically) {
            Text(title, Modifier.weight(1f), fontSize = 16.sp, fontWeight = FontWeight.SemiBold)
            Text(detail, fontSize = 12.sp, color = Palette.textTertiary)
        }
        rows()
    }
}

@Composable
private fun CardRow(modifier: Modifier, onClick: () -> Unit, content: @Composable RowScope.() -> Unit) {
    HorizontalDivider(color = Palette.divider)
    Row(
        modifier.fillMaxWidth().heightIn(min = 48.dp).clickable(role = Role.Button, onClick = onClick).padding(horizontal = 14.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) { content() }
}

@Composable
private fun WorkspaceRow(t: Terminal, loop: LoopInfo?, onClick: () -> Unit) {
    val state = t.state
    CardRow(Modifier.testTag("terminal.${t.id}"), onClick) {
        Dot(state.color)
        Column(Modifier.weight(1f).padding(vertical = 6.dp), verticalArrangement = Arrangement.spacedBy(1.dp)) {
            Text(t.title, maxLines = 1, overflow = TextOverflow.Ellipsis)
            t.place?.let { Text(it, fontSize = 12.sp, color = Palette.textTertiary, maxLines = 1, overflow = TextOverflow.Ellipsis) }
            loop?.let { LoopBadge(it) }
        }
        if (state != TaskState.UNKNOWN) Text(state.title.lowercase(), fontSize = 12.5.sp, color = state.color)
        Icon(Icons.chevron, null, Modifier.size(16.dp), tint = Palette.textTertiary)
    }
}

/** The tablet's Tasks: grouped by state, Needs you first. */
@Composable
fun StateList(model: NextModel, selected: String?, select: (String) -> Unit) {
    val live = model.terminals.filter { it.live }
    val groups = TaskState.entries.associateWith { s -> live.filter { it.state == s } }.filterValues { it.isNotEmpty() }
    LazyColumn(
        Modifier.fillMaxSize(),
        contentPadding = PaddingValues(start = 12.dp, end = 12.dp, bottom = 16.dp),
        verticalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        for ((state, list) in groups) {
            item(key = "state-$state") {
                SectionHeader(state.title, if (state == TaskState.UNKNOWN) Palette.textTertiary else state.color, list.size)
            }
            items(list, key = { it.id }) { t -> StateRow(t, model.loops[t.id], t.id == selected) { select(t.id) } }
        }
        if (live.isEmpty()) item { Text("No Tasks yet. Start one with +.", Modifier.padding(8.dp), color = Palette.textSecondary) }
    }
}

@Composable
private fun StateRow(t: Terminal, loop: LoopInfo?, picked: Boolean, onClick: () -> Unit) {
    Row(
        Modifier
            .fillMaxWidth()
            .heightIn(min = 48.dp)
            .clip(RoundedCornerShape(10.dp))
            .background(if (picked) Palette.picked else Color.Transparent)
            .semantics { selected = picked }
            .clickable(onClick = onClick)
            .padding(10.dp)
            .testTag("terminal.${t.id}"),
        horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Dot(t.state.color, modifier = Modifier.padding(top = 5.dp))
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(3.dp)) {
            Text(t.path(), fontSize = 12.sp, color = Palette.textSecondary, maxLines = 1, overflow = TextOverflow.Ellipsis)
            Text(t.title, fontSize = 14.sp, lineHeight = 19.sp, color = Palette.text)
            loop?.let { LoopBadge(it) }
        }
    }
}

@Composable
private fun LoopBadge(loop: LoopInfo) {
    Text(
        LoopText.badge(loop), fontSize = 11.sp,
        color = if (loop.reason == "limit") Theme.attention else Palette.textTertiary,
    )
}
