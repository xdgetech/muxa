package tech.xdge.muxa.app.ui

import androidx.compose.runtime.getValue
import androidx.compose.runtime.setValue
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.RadioButton
import androidx.compose.material3.TextButton
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import tech.xdge.muxa.app.AppModel
import tech.xdge.muxa.app.Selection
import tech.xdge.muxa.app.back
import tech.xdge.muxa.app.paneTask
import tech.xdge.muxa.app.select
import tech.xdge.muxa.app.showTask
import tech.xdge.muxa.app.ReaderTab
import tech.xdge.muxa.app.isAgentSide
import tech.xdge.muxa.protocol.TaskRecord
import tech.xdge.muxa.protocol.TaskStatus
import tech.xdge.muxa.session.InboxNeed
import tech.xdge.muxa.session.need
import tech.xdge.muxa.session.TaskItem
import tech.xdge.muxa.session.hasHandoff
import tech.xdge.muxa.session.taskItem
import androidx.compose.ui.semantics.testTagsAsResourceId
import androidx.compose.ui.semantics.semantics
import androidx.compose.material3.Surface
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.ui.window.DialogProperties
import androidx.compose.ui.window.Dialog
import tech.xdge.muxa.app.openTaskRow

/**
 * Tasks mode (ADR 0035): the Routines first, then every Task by group. The row of what the
 * detail shows is highlighted: the picked Task, or the Task whose pane is shown.
 */
@Composable
fun TaskList(model: AppModel, modifier: Modifier) {
    val selected = model.selectedTask ?: model.paneTask
    Column(modifier) {
        val sections = model.listedTasks
        Box(Modifier.fillMaxWidth().weight(1f)) {
            LazyColumn(Modifier.fillMaxSize().testTag("task-list")) {
                if (model.canUseRoutines) item(key = "routines") { RoutinesRow(model) }
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
                        TaskRow(item, item.record.id == selected) { model.openTaskRow(item) }
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

/** Opens the Routines in the detail, with how many the host has. */
@Composable
private fun RoutinesRow(model: AppModel) {
    val current = model.selection == Selection.Routines || model.selection is Selection.Routine
    SidebarRow("Routines", Icons.routine, "open-routines", current, "${model.state.routines.size}") { model.select(Selection.Routines) }
}

@Composable
private fun TaskRow(item: TaskItem, current: Boolean, open: () -> Unit) {
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
private fun AgentLine(item: TaskItem) {
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

/** A Task's detail, or "Pick a Task." without one. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun TaskDetail(model: AppModel, record: TaskRecord?, modifier: Modifier, onClose: (() -> Unit)? = null) {
    val phone = LocalPhoneLayout.current
    Column(modifier) {
        TopAppBar(
            title = { Text(record?.title ?: "", maxLines = 1, overflow = TextOverflow.Ellipsis) },
            navigationIcon = {
                if (onClose != null) {
                    IconButton(onClose, Modifier.testTag("task-info-done")) { Icon(Icons.close, "Done") }
                } else if (phone && record != null) {
                    IconButton(model::back, Modifier.testTag("task-back")) { Icon(Icons.back, "Tasks") }
                }
            },
        )
        if (record == null) {
            Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) { Text("Pick a Task.") }
            return@Column
        }
        val item = taskItem(record, model.state.sessions)
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
            RestartRecoverySection(model, record)
            LoopSection(model, record, item)
            Field("Goal", record.goal)
            Field("Workspace", record.workspace)
            if (model.state.canUseWorkers || record.worker != null) Field("Worker", record.worker ?: "Workspace default")
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

/** A Task's detail over its pane, from More (ADR 0044). An action that opens a view closes it. */
@OptIn(androidx.compose.ui.ExperimentalComposeUiApi::class)
@Composable
fun TaskInfoDialog(model: AppModel, id: String) {
    val close = { model.taskInfo = null; model.refocusPane() }
    val record = model.state.tasks?.records?.firstOrNull { it.id == id }
    Dialog(onDismissRequest = close, properties = DialogProperties(usePlatformDefaultWidth = false)) {
        // A dialog is its own window: it needs the tag setting the main window has (Root).
        Surface(Modifier.fillMaxSize().semantics { testTagsAsResourceId = true }, color = MaterialTheme.colorScheme.background) {
            TaskDetail(model, record, Modifier.fillMaxSize().navigationBarsPadding().testTag("task-info"), onClose = close)
        }
    }
}

/** Where to go from a Task: its session's pane, its Chat, the Reader, or a restarted agent. None without a session. */
@OptIn(androidx.compose.foundation.layout.ExperimentalLayoutApi::class)
@Composable
private fun TaskActions(model: AppModel, record: TaskRecord, item: TaskItem) {
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
            OutlinedButton({ model.restartingTask = record.id }, Modifier.testTag("task-restart-agent"), enabled = record.restartOperation == null) {
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
internal fun Field(label: String, value: String, mono: Boolean = false) {
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
fun TaskSessionBadge(item: TaskItem) {
    Column(horizontalAlignment = Alignment.End) {
        Text(item.group.title, style = MaterialTheme.typography.labelSmall)
        val since = if (item.need == InboxNeed.LOOP) item.record.loop?.since else item.agent?.since
        since?.let { Text(AgentSummary.age(it), style = MaterialTheme.typography.labelSmall) }
        LoopBadge(item.record)
    }
}

