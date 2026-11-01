package tech.xdge.muxa.app.ui

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.VerticalDivider
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.testTagsAsResourceId
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import tech.xdge.muxa.app.AppModel
import tech.xdge.muxa.app.ReaderTab
import tech.xdge.muxa.app.isAgentSide
import tech.xdge.muxa.protocol.TaskRecord
import tech.xdge.muxa.protocol.TaskStatus
import tech.xdge.muxa.session.InboxItem
import tech.xdge.muxa.session.inboxItem

/**
 * The host's Tasks by Inbox group, and one Task's detail. Like the Reader it covers the
 * list and the pane; on a phone the detail is pushed over the list, elsewhere it sits beside it.
 */
@Composable
fun TasksScreen(model: AppModel) {
    val phone = LocalPhoneLayout.current
    val layout = LocalWindowLayout.current
    val selected = model.selectedTask?.let { id -> model.state.tasks?.record(id) }
    Dialog(onDismissRequest = model::closeTasks, properties = DialogProperties(usePlatformDefaultWidth = false, decorFitsSystemWindows = false)) {
        DialogStatusBar()
        // A dialog is its own window: it needs the tag setting the main window has (Root).
        Surface(Modifier.fillMaxSize().semantics { testTagsAsResourceId = true }, color = MaterialTheme.colorScheme.background) {
            val list = @Composable { modifier: Modifier -> TaskList(model, selected?.id, modifier) }
            val detail = @Composable { modifier: Modifier -> TaskDetail(model, selected, modifier) }
            when {
                phone -> {
                    if (selected == null) list(Modifier.fillMaxSize()) else detail(Modifier.fillMaxSize())
                    BackHandler(selected != null) { model.selectedTask = null }
                }
                // Half open like a book: the list left of the hinge, the detail right of it.
                layout is WindowLayout.Book -> Row(Modifier.fillMaxSize()) {
                    list(Modifier.width(layout.listWidth.dp).fillMaxHeight())
                    Spacer(Modifier.width(layout.gap.dp))
                    detail(Modifier.weight(1f).fillMaxHeight())
                }
                else -> Row(Modifier.fillMaxSize()) {
                    list(Modifier.width(360.dp).fillMaxHeight())
                    VerticalDivider()
                    detail(Modifier.weight(1f).fillMaxHeight())
                }
            }
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun TaskList(model: AppModel, selected: String?, modifier: Modifier) {
    Column(modifier) {
        TopAppBar(
            title = { Text("Tasks") },
            navigationIcon = { IconButton(model::closeTasks, Modifier.testTag("tasks-done")) { Icon(Icons.close, "Done") } },
        )
        val sections = model.inbox
        LazyColumn(Modifier.fillMaxWidth().weight(1f)) {
            for (section in sections) {
                item(key = "g-${section.group}") {
                    Text(
                        section.group.title,
                        Modifier.padding(start = 16.dp, top = 16.dp, bottom = 4.dp).testTag("inbox-${section.group.name}"),
                        style = MaterialTheme.typography.labelLarge,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
                items(section.items, key = { it.record.id }) { item ->
                    TaskRow(item, item.record.id == selected) { model.selectedTask = item.record.id }
                }
            }
            if (model.state.tasks?.truncated == true) {
                item { Text("Only the newest Tasks are listed.", Modifier.padding(16.dp), style = MaterialTheme.typography.bodySmall) }
            }
        }
        if (sections.isEmpty()) {
            Box(Modifier.fillMaxSize().padding(32.dp), contentAlignment = Alignment.Center) {
                Text(if (model.state.tasks == null) "Loading Tasks…" else "No Tasks on this host yet.")
            }
        }
    }
}

@Composable
private fun TaskRow(item: InboxItem, current: Boolean, open: () -> Unit) {
    val r = item.record
    Row(
        Modifier.fillMaxWidth()
            .background(if (current) MaterialTheme.colorScheme.secondaryContainer else MaterialTheme.colorScheme.surface)
            .clickable(onClick = open)
            .padding(horizontal = 16.dp, vertical = 10.dp)
            .testTag("task-${r.id}"),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Column(Modifier.weight(1f)) {
            Text(r.title, fontWeight = FontWeight.SemiBold, maxLines = 2, overflow = TextOverflow.Ellipsis)
            Text(
                listOfNotNull(r.workspace, r.project).joinToString(" · "),
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                maxLines = 1,
            )
        }
        Column(horizontalAlignment = Alignment.End) {
            AgentLine(item)
            StatusLabel(r.status)
        }
    }
}

/** The agent's state with its age, only when an adapter reports one; else "No agent". */
@Composable
private fun AgentLine(item: InboxItem) {
    val pane = item.agentPane
    if (item.agent != null && pane != null) {
        AgentBadge(pane)
    } else {
        Text("No agent", style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
    }
}

@Composable
private fun StatusLabel(status: TaskStatus) {
    Text(statusTitle(status), style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
}

/** "Open", "In review", "Done", "Dropped"; a status from a newer bridge keeps its name. */
fun statusTitle(status: TaskStatus): String = when (status) {
    TaskStatus.OPEN -> "Open"
    TaskStatus.REVIEW -> "In review"
    TaskStatus.DONE -> "Done"
    TaskStatus.DROPPED -> "Dropped"
    else -> status.raw
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun TaskDetail(model: AppModel, record: TaskRecord?, modifier: Modifier) {
    val phone = LocalPhoneLayout.current
    Column(modifier) {
        TopAppBar(
            title = { Text(record?.title ?: "", maxLines = 1, overflow = TextOverflow.Ellipsis) },
            navigationIcon = {
                if (phone) IconButton({ model.selectedTask = null }, Modifier.testTag("task-back")) { Icon(Icons.back, "Tasks") }
            },
        )
        if (record == null) {
            Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) { Text("Pick a Task.") }
            return@Column
        }
        val item = inboxItem(record, model.state.sessions)
        Column(
            Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(16.dp).testTag("task-detail"),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                Text(item.group.title, style = MaterialTheme.typography.titleSmall)
                StatusLabel(record.status)
                Spacer(Modifier.weight(1f))
                AgentLine(item)
            }
            record.note?.let { Text(it, color = Theme.failure, modifier = Modifier.testTag("task-note")) }
            TaskActions(model, record, item)
            Field("Goal", record.goal)
            Field("Workspace", record.workspace)
            record.project?.let { Field("Project", it) }
            record.repo?.let { Field("Repository", it, mono = true) }
            record.branch?.let { Field("Branch", it, mono = true) }
            record.dir?.let { Field("Directory", it, mono = true) }
            (item.session?.name ?: record.session)?.let { Field("Session", it, mono = true) }
            Field("Updated", AgentSummary.age(record.updatedAt) + " ago")
        }
    }
}

/** Where to go from a Task: its session's pane, its Chat, or the Reader. None without a session. */
@OptIn(androidx.compose.foundation.layout.ExperimentalLayoutApi::class)
@Composable
private fun TaskActions(model: AppModel, record: TaskRecord, item: InboxItem) {
    val session = item.session ?: return
    val agent = item.agentPane ?: session.panes.firstOrNull { it.isAgentSide }
    val canChat = agent != null && model.state.canChat && agent.id !in model.state.unsupportedChats
    FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        OutlinedButton({ model.openTask(record, AppModel.PaneView.AGENT) }, Modifier.testTag("task-open-pane")) {
            Icon(Icons.agent, null)
            Text("Terminal", Modifier.padding(start = 6.dp))
        }
        if (canChat) {
            OutlinedButton({ model.openTask(record, AppModel.PaneView.CHAT) }, Modifier.testTag("task-open-chat")) {
                Icon(Icons.chat, null)
                Text("Chat", Modifier.padding(start = 6.dp))
            }
        }
        if (model.state.canReadDocs) {
            OutlinedButton({ model.readTask(record, ReaderTab.DOCS) }, Modifier.testTag("task-open-brief")) {
                Icon(Icons.document, null)
                Text("TASK.md", Modifier.padding(start = 6.dp))
            }
        }
        if (model.state.canShowChanges && session.worktree != null) {
            OutlinedButton({ model.readTask(record, ReaderTab.CHANGES) }, Modifier.testTag("task-open-changes")) {
                Icon(Icons.changes, null)
                Text("Changes", Modifier.padding(start = 6.dp))
            }
        }
    }
}

@Composable
private fun Field(label: String, value: String, mono: Boolean = false) {
    Column {
        Text(label, style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
        SelectionContainer {
            Text(value, fontFamily = if (mono) FontFamily.Monospace else null)
        }
    }
}
