package tech.xdge.muxa.app.ui

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.gestures.detectHorizontalDragGestures
import androidx.compose.ui.input.pointer.pointerInput
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
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.RadioButton
import androidx.compose.material3.TextButton
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.VerticalDivider
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
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
import tech.xdge.muxa.app.shortcut
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.runtime.LaunchedEffect
import tech.xdge.muxa.session.InboxNeed
import tech.xdge.muxa.session.need
import tech.xdge.muxa.session.InboxItem
import tech.xdge.muxa.session.defaultPane
import tech.xdge.muxa.session.hasHandoff
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
        Surface(Modifier.fillMaxSize().onPreviewKeyEvent { model.shortcut(it.nativeKeyEvent) }.semantics { testTagsAsResourceId = true }, color = MaterialTheme.colorScheme.background) {
            Column(Modifier.fillMaxSize()) {
                Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
                    TextButton({ model.inboxList = false; model.selectedTask = null }, Modifier.testTag("inbox-triage")) { Text("Triage") }
                    TextButton({ model.inboxList = true; model.selectedTask = null }, Modifier.testTag("inbox-list-mode")) { Text("List") }
                    if (!model.inboxList && selected == null && model.state.tasks?.workspaces?.isNotEmpty() == true) {
                        TextButton(model::startNewTaskRecord, Modifier.testTag("new-task-record")) { Text("New Task") }
                    }
                    TextButton(model::closeTasks, Modifier.testTag("inbox-close")) { Text("Close Inbox") }
                }
                if (!model.inboxList && selected == null) {
                    Triage(model)
                } else {
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
    }
    // After the screen, so these windows open over it.
    model.taskError?.let { error ->
        AlertDialog(
            onDismissRequest = { model.taskError = null },
            title = { Text("Can't Change the Task") },
            text = { Text(error, Modifier.testTag("task-error-message")) },
            confirmButton = { TextButton({ model.taskError = null }) { Text("OK") } },
        )
    }
    RestartAgentDialog(model)
    RestartErrorDialog(model)
    StartLoopDialog(model)
    LoopErrorDialog(model)
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun TaskList(model: AppModel, selected: String?, modifier: Modifier) {
    Column(modifier) {
        TopAppBar(
            title = { Text("Inbox") },
            navigationIcon = { IconButton(model::closeTasks, Modifier.testTag("tasks-done")) { Icon(Icons.close, "Done") } },
            actions = {
                if (model.state.tasks?.workspaces?.isNotEmpty() == true) {
                    IconButton(model::startNewTaskRecord, Modifier.testTag("new-task-record")) { Icon(Icons.add, "New Task") }
                }
            },
        )
        val sections = model.inbox
        Box(Modifier.fillMaxWidth().weight(1f)) {
            LazyColumn(Modifier.fillMaxSize().testTag("task-list")) {
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
            LoopBadge(r)
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
                StatusMenu(model, record)
                Spacer(Modifier.weight(1f))
                // The group already says "No agent" otherwise.
                if (item.agent != null) AgentLine(item)
            }
            record.note?.let { Text(it, color = Theme.failure, modifier = Modifier.testTag("task-note")) }
            TaskActions(model, record, item)
            LoopSection(model, record, item)
            Field("Goal", record.goal)
            Field("Workspace", record.workspace)
            record.project?.let { Field("Project", it) }
            record.repo?.let { Field("Repository", it, mono = true) }
            record.branch?.let { Field("Branch", it, mono = true) }
            record.dir?.let { Field("Directory", it, mono = true) }
            (item.session?.name ?: record.session)?.let { Field("Session", it, mono = true) }
            Field("Updated", AgentSummary.age(record.updatedAt) + " ago")
            RunsSection(record, item)
        }
    }
}

/** Where to go from a Task: its session's pane, its Chat, the Reader, or a restarted agent. None without a session. */
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
            if (record.hasHandoff) {
                OutlinedButton({ model.readTask(record, ReaderTab.DOCS, AppModel.HANDOFF_DOC) }, Modifier.testTag("task-open-handoff")) {
                    Icon(Icons.document, null)
                    Text("HANDOFF.md", Modifier.padding(start = 6.dp))
                }
            }
        }
        if (model.state.canShowChanges && session.worktree != null) {
            OutlinedButton({ model.readTask(record, ReaderTab.CHANGES) }, Modifier.testTag("task-open-changes")) {
                Icon(Icons.changes, null)
                Text("Changes", Modifier.padding(start = 6.dp))
            }
        }
        if (model.canRestartAgents) {
            OutlinedButton({ model.restartingTask = record.id }, Modifier.testTag("task-restart-agent")) {
                Icon(Icons.restartAgent, null)
                Text("Restart agent", Modifier.padding(start = 6.dp))
            }
        }
    }
}

/** The status the user sets; never follows the agent (ADR 0015). */
@Composable
private fun StatusMenu(model: AppModel, record: TaskRecord) {
    var open by remember { mutableStateOf(false) }
    Box {
        TextButton({ open = true }, Modifier.testTag("task-status")) { Text("Status: ${statusTitle(record.status)}") }
        DropdownMenu(open, { open = false }) {
            for (status in listOf(TaskStatus.OPEN, TaskStatus.REVIEW, TaskStatus.DONE, TaskStatus.DROPPED)) {
                DropdownMenuItem(
                    { Text(statusTitle(status)) },
                    {
                        open = false
                        model.setStatus(record, status)
                    },
                    leadingIcon = { RadioButton(record.status == status, null) },
                    modifier = Modifier.testTag("status-${status.raw}"),
                )
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

val InboxNeed.title: String get() = when (this) {
    InboxNeed.NONE -> "Nothing needs you"
    InboxNeed.ANSWER -> "Waits for your answer"
    InboxNeed.AGENT_ERROR -> "The agent stopped with an error"
    InboxNeed.LOOP -> "Loop needs attention"
    InboxNeed.REVIEW -> "Ready for review"
    InboxNeed.NO_AGENT -> "No agent state is available"
    InboxNeed.NO_SESSION -> "No session runs this Task"
}

@Composable
fun TaskSessionBadge(item: InboxItem) {
    Column(horizontalAlignment = Alignment.End) {
        Text(item.group.title, style = MaterialTheme.typography.labelSmall)
        val since = if (item.need == InboxNeed.LOOP) item.record.loop?.since else item.agent?.since
        since?.let { Text(AgentSummary.age(it), style = MaterialTheme.typography.labelSmall) }
        LoopBadge(item.record)
    }
}

@Composable
private fun Triage(model: AppModel) {
    val queue = model.inboxQueue
    val item = queue.firstOrNull { it.record.id == model.inboxFocus } ?: queue.firstOrNull()
    LaunchedEffect(item?.record?.id) { if (item != null) model.inboxFocus = item.record.id }
    if (model.inboxEnded || item == null) {
        Column(Modifier.fillMaxSize().padding(24.dp).testTag("triage-end"), verticalArrangement = Arrangement.spacedBy(16.dp)) {
            Text("Nothing else needs you.", style = MaterialTheme.typography.headlineSmall)
            Text("${model.inboxSkipped.size} skipped · ${model.inboxSnoozedCount} snoozed")
            TextButton({ model.inboxEnded = false; model.inboxFocus = queue.firstOrNull()?.record?.id; model.inboxSkipped = emptySet() }) { Text("Start over") }
            TextButton({ model.inboxList = true }) { Text("Show List") }
            TextButton({ model.inboxSnoozes = emptyMap(); model.inboxEnded = false }) { Text("Show snoozed") }
        }
        return
    }
    Column(Modifier.fillMaxSize().padding(16.dp).testTag("triage-card").pointerInput(item.record.id) {
        var distance = 0f
        detectHorizontalDragGestures(
            onDragStart = { distance = 0f },
            onHorizontalDrag = { _, amount -> distance += amount },
            onDragEnd = { if (distance < -120f) model.stepInbox(1) },
        )
    }) {
        Column(Modifier.weight(1f).verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            Text("${queue.indexOf(item) + 1} of ${queue.size}")
            Text(item.group.title, style = MaterialTheme.typography.labelLarge)
            Text(item.record.title, style = MaterialTheme.typography.headlineSmall)
            Text(listOfNotNull(item.record.workspace, item.record.project, item.session?.name).joinToString(" · "))
            TaskSessionBadge(item)
            Text(item.need.title, Modifier.testTag("triage-need"), style = MaterialTheme.typography.titleLarge)
            Text(item.record.goal)
            val target = item.session
            if (model.isConnected && target?.defaultPane != null) {
                if (model.state.canChat && item.agentPane != null && item.agentPane?.id !in model.state.unsupportedChats) {
                    TextButton({ model.openTask(item.record, AppModel.PaneView.CHAT) }) { Text("Open Chat") }
                }
                TextButton({ model.openTask(item.record, AppModel.PaneView.AGENT) }) { Text("Open Terminal") }
                if (model.state.canReadDocs) {
                    TextButton({ model.readTask(item.record, ReaderTab.DOCS) }) { Text("TASK.md") }
                    if (item.record.loop?.lastCheck != null) {
                        TextButton({ model.readTask(item.record, ReaderTab.DOCS, ".muxa/CHECK.md") }) { Text("Check log") }
                    }
                }
                if (model.state.canReadDocs && model.state.canShowChanges && target.worktree != null) {
                    TextButton({ model.readTask(item.record, ReaderTab.CHANGES) }) { Text("Changes") }
                }
            }
            TextButton({ model.selectedTask = item.record.id }, Modifier.testTag("triage-detail")) { Text("Task detail") }
            if (model.inboxSnoozedCount > 0) {
                TextButton({ model.inboxSnoozes = emptyMap() }) { Text("Show snoozed (${model.inboxSnoozedCount})") }
            }
            queue.drop(queue.indexOf(item) + 1).take(2).forEach { next ->
                TextButton({ model.inboxFocus = next.record.id }) { Text("Up next: ${next.record.title} · ${next.need.title}") }
            }
        }
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
            TextButton({ model.stepInbox(-1) }) { Text("Previous") }
            TextButton({ model.openTask(item.record, if (model.state.canChat && item.agentPane != null) AppModel.PaneView.CHAT else AppModel.PaneView.AGENT) }, enabled = model.isConnected && item.session?.defaultPane != null) { Text("Open") }
            TextButton(model::snoozeInbox, Modifier.testTag("triage-snooze")) { Text("Snooze") }
            TextButton({ model.stepInbox(1) }, Modifier.testTag("triage-next")) { Text("Next") }
        }
    }
}
