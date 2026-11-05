package tech.xdge.muxa.app.ui

import androidx.compose.runtime.getValue
import androidx.compose.runtime.setValue
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.background
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import tech.xdge.muxa.app.AppModel
import tech.xdge.muxa.protocol.Session
import tech.xdge.muxa.session.RepoGroup
import tech.xdge.muxa.session.defaultPane

/** Sessions mode: the manual sessions by repo, with each repo's Ask and New Task (ADR 0025, 0031). */
@Composable
fun SessionList(model: AppModel, modifier: Modifier) {
    val numbers = model.visibleSessions.withIndex().associate { it.value.id to it.index + 1 }
    // The row of the pane the detail shows.
    val shown = model.currentSession?.id?.takeIf { model.paneShown }
    LazyColumn(modifier.testTag("session-list")) {
        if (model.noManualSessions) {
            item(key = "sessions-empty") {
                Text(
                    "No manual sessions. Open a Task's session from Inbox or Tasks.",
                    Modifier.padding(horizontal = 16.dp, vertical = 8.dp),
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
        }
        for (group in model.sessionGroups) {
            item(key = "repo-${group.repo}") { RepoHeader(group, newTaskAction(model, group), askAction(model, group)) }
            items(group.sessions, key = { it.id }) { s ->
                SessionRow(model, s, current = s.id == shown, number = numbers[s.id])
            }
        }
    }
}

/** Only a repo the host knows the path of can get a task. */
private fun newTaskAction(model: AppModel, group: RepoGroup): (() -> Unit)? {
    val dir = group.sessions.firstOrNull()?.repoDir
    if (!model.canCreateTasks || group.repo == null || dir == null) return null
    return { model.startNewTask(dir) }
}

/** Ask about a repo: only one a workspace holds, on a host with ask sessions (ADR 0031). */
private fun askAction(model: AppModel, group: RepoGroup): (() -> Unit)? {
    val dir = group.sessions.firstOrNull()?.repoDir
    if (group.repo == null || dir == null || model.askWorkspace(dir) == null) return null
    return { model.askAbout(dir) }
}

/** A repo's section header; sessions without a repo tag come last. */
@Composable
private fun RepoHeader(group: RepoGroup, newTask: (() -> Unit)?, ask: (() -> Unit)?) {
    SectionHeader(group.repo ?: "Other Sessions") {
        if (ask != null && group.repo != null) {
            IconButton(ask, Modifier.testTag("ask-${group.repo}")) { Icon(Icons.ask, "Ask about ${group.repo}") }
        }
        if (newTask != null && group.repo != null) {
            IconButton(newTask, Modifier.testTag("new-task-${group.repo}")) { Icon(Icons.add, "New Task in ${group.repo}") }
        }
    }
}

@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun SessionRow(model: AppModel, s: Session, current: Boolean, number: Int?) {
    var menu by remember { mutableStateOf(false) }
    val item = model.inbox.flatMap { it.items }.firstOrNull { it.session?.id == s.id }
    Box {
        Row(
            Modifier.fillMaxWidth()
                .background(if (current) MaterialTheme.colorScheme.secondaryContainer else MaterialTheme.colorScheme.surface)
                .combinedClickable(onClick = { model.open(s) }, onLongClick = { if (model.canClose(s) || model.canCloseAsk(s) || item != null) menu = true })
                .padding(horizontal = 16.dp, vertical = 10.dp)
                .testTag("session-${s.name}"),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Column(Modifier.weight(1f)) {
                Text(item?.record?.title ?: s.name, fontWeight = if (current) FontWeight.SemiBold else FontWeight.Normal, maxLines = 1, overflow = TextOverflow.Ellipsis)
                listOfNotNull(if (item != null) s.name else null, s.branch).joinToString(" · ").takeIf { it.isNotEmpty() }?.let {
                    Text(it, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant, maxLines = 1)
                }
            }
            if (item != null) TaskSessionBadge(item) else s.defaultPane?.let { AgentBadge(it) }
            number?.takeIf { it <= 9 }?.let {
                Text("^⇧$it", style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
        }
        DropdownMenu(menu, { menu = false }) {
            if (item != null) DropdownMenuItem({ Text("Show in Inbox") }, { menu = false; model.showInbox(item.record.id) })
            if (model.canClose(s)) DropdownMenuItem({ Text("Close Task…") }, { menu = false; model.closingTask = s }, leadingIcon = { Icon(Icons.closeTask, null) })
            if (model.canCloseAsk(s)) DropdownMenuItem({ Text("Close Session…") }, { menu = false; model.closingAsk = s }, leadingIcon = { Icon(Icons.closeTask, null) })
        }
    }
}
