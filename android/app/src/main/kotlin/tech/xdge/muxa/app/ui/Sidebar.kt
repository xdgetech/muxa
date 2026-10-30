package tech.xdge.muxa.app.ui

import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.background
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import tech.xdge.muxa.app.AppModel
import tech.xdge.muxa.protocol.Session
import tech.xdge.muxa.session.Host
import tech.xdge.muxa.session.RepoGroup
import tech.xdge.muxa.session.defaultPane

/** Saved hosts, and the connected host's sessions below them. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun Sidebar(model: AppModel, onEdit: (HostDraft) -> Unit, onHide: (() -> Unit)?, modifier: Modifier = Modifier) {
    Column(modifier) {
        TopAppBar(
            title = { Text("Muxa") },
            navigationIcon = {
                if (onHide != null && !LocalPhoneLayout.current) {
                    IconButton(onHide, Modifier.testTag("hide-list")) { Icon(Icons.list, "Hide List") }
                }
            },
            actions = { IconButton({ onEdit(HostDraft.new()) }) { Icon(Icons.add, "Add Host") } },
        )
        val numbers = model.visibleSessions.withIndex().associate { it.value.id to it.index + 1 }
        LazyColumn(Modifier.fillMaxWidth()) {
            // No empty header on first launch; the detail says what to do.
            if (model.hostList.hosts.isNotEmpty()) {
                item { SectionHeader("Hosts") }
                items(model.hostList.hosts, key = { it.id }) { host ->
                    HostRow(model, host, onEdit)
                }
            }
            if (model.isConnected) {
                for (group in model.sessionGroups) {
                    item(key = "repo-${group.repo}") { RepoHeader(group, newTaskAction(model, group)) }
                    items(group.sessions, key = { it.id }) { s ->
                        SessionRow(model, s, current = s.id == model.currentSession?.id, number = numbers[s.id])
                    }
                }
            }
            model.storageError?.let { error ->
                item { Text(error, Modifier.padding(16.dp), color = Theme.failure) }
            }
        }
    }
}

/** Only a repo the host knows the path of can get a task. */
private fun newTaskAction(model: AppModel, group: RepoGroup): (() -> Unit)? {
    val dir = group.sessions.firstOrNull()?.repoDir
    if (!model.canStartTasks || group.repo == null || dir == null) return null
    return { model.startNewTask(dir) }
}

@Composable
private fun SectionHeader(text: String, trailing: @Composable () -> Unit = {}) {
    Row(Modifier.fillMaxWidth().padding(start = 16.dp, end = 4.dp, top = 16.dp), verticalAlignment = Alignment.CenterVertically) {
        Text(text, style = MaterialTheme.typography.labelLarge, color = MaterialTheme.colorScheme.onSurfaceVariant, modifier = Modifier.weight(1f))
        trailing()
    }
}

@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun HostRow(model: AppModel, host: Host, onEdit: (HostDraft) -> Unit) {
    var menu by remember { mutableStateOf(false) }
    val current = host.id == model.currentHostId
    Box {
        Row(
            Modifier.fillMaxWidth()
                .combinedClickable(onClick = { model.connect(host.id) }, onLongClick = { menu = true })
                .padding(horizontal = 16.dp, vertical = 10.dp)
                .testTag("host-${host.title}"),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Icon(Icons.host, null, tint = MaterialTheme.colorScheme.onSurfaceVariant)
            Column(Modifier.padding(start = 12.dp).weight(1f)) {
                Text(host.title)
                Text(
                    "${host.target.user}@${host.target.host}",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    maxLines = 1,
                )
            }
            when {
                current && (model.connecting || model.addingKey) -> CircularProgressIndicator(Modifier.size(18.dp), strokeWidth = 2.dp)
                current && model.isConnected -> Box(
                    Modifier.size(8.dp).background(Theme.connected, CircleShape).semantics { contentDescription = "Connected" },
                )
            }
            IconButton({ menu = true }) { Icon(Icons.more, "Host Actions") }
        }
        DropdownMenu(menu, { menu = false }) {
            DropdownMenuItem({ Text("Edit") }, { menu = false; onEdit(HostDraft.of(host)) }, leadingIcon = { Icon(Icons.edit, null) })
            DropdownMenuItem(
                { Text("Delete", color = Theme.failure) },
                { menu = false; model.hosts.delete(host.id) },
                leadingIcon = { Icon(Icons.delete, null, tint = Theme.failure) },
            )
        }
    }
}

/** A repo's section header; sessions without a repo tag come last. */
@Composable
private fun RepoHeader(group: RepoGroup, newTask: (() -> Unit)?) {
    SectionHeader(group.repo ?: "Other Sessions") {
        if (newTask != null && group.repo != null) {
            IconButton(newTask, Modifier.testTag("new-task-${group.repo}")) { Icon(Icons.add, "New Task in ${group.repo}") }
        }
    }
}

@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun SessionRow(model: AppModel, s: Session, current: Boolean, number: Int?) {
    var menu by remember { mutableStateOf(false) }
    Box {
        Row(
            Modifier.fillMaxWidth()
                .background(if (current) MaterialTheme.colorScheme.secondaryContainer else MaterialTheme.colorScheme.surface)
                .combinedClickable(onClick = { model.open(s) }, onLongClick = { if (model.canClose(s)) menu = true })
                .padding(horizontal = 16.dp, vertical = 10.dp)
                .testTag("session-${s.name}"),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Column(Modifier.weight(1f)) {
                Text(s.name, fontWeight = if (current) FontWeight.SemiBold else FontWeight.Normal, maxLines = 1, overflow = TextOverflow.Ellipsis)
                s.branch?.let {
                    Text(it, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant, maxLines = 1)
                }
            }
            s.defaultPane?.let { AgentBadge(it) }
            number?.takeIf { it <= 9 }?.let {
                Text("^⇧$it", style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
        }
        DropdownMenu(menu, { menu = false }) {
            DropdownMenuItem({ Text("Close Task…") }, { menu = false; model.closingTask = s }, leadingIcon = { Icon(Icons.closeTask, null) })
        }
    }
    Spacer(Modifier.width(0.dp))
}
