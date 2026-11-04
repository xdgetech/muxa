package tech.xdge.muxa.app.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.SegmentedButton
import androidx.compose.material3.SegmentedButtonDefaults
import androidx.compose.material3.SingleChoiceSegmentedButtonRow
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import tech.xdge.muxa.app.AppModel
import tech.xdge.muxa.app.SidebarMode
import tech.xdge.muxa.app.selectMode

/**
 * The sidebar shows one mode: Inbox, Tasks or Sessions (ADR 0035). Above it sit New Task,
 * Settings and one status row for the connection; the hosts themselves live in Settings.
 */
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
            actions = {
                if (model.isConnected && model.canCreateTasks) {
                    IconButton({ model.startNewTask() }, Modifier.testTag("sidebar-new-task")) { Icon(Icons.add, "New Task") }
                }
                IconButton({ model.showSettings = true }, Modifier.testTag("open-settings")) { Icon(Icons.settings, "Settings") }
            },
        )
        if (model.isConnected) ModePicker(model)
        StatusRow(model)
        val body = Modifier.fillMaxWidth().weight(1f)
        when {
            !model.isConnected -> NotConnected(model, onEdit, body)
            model.mode == SidebarMode.INBOX -> ItemList(model, body)
            model.mode == SidebarMode.TASKS -> TaskList(model, body)
            else -> SessionList(model, body)
        }
        model.storageError?.let { error -> Text(error, Modifier.padding(16.dp), color = Theme.failure) }
    }
}

/** The segmented control under the title. The Inbox segment counts the waiting items. */
@Composable
private fun ModePicker(model: AppModel) {
    val modes = SidebarMode.entries
    SingleChoiceSegmentedButtonRow(Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 4.dp).testTag("sidebar-mode")) {
        modes.forEachIndexed { i, mode ->
            val count = if (mode == SidebarMode.INBOX) model.inboxItems.size else 0
            SegmentedButton(
                selected = model.mode == mode,
                onClick = { model.selectMode(mode) },
                shape = SegmentedButtonDefaults.itemShape(i, modes.size),
                modifier = Modifier.testTag("sidebar-mode-${mode.tag}"),
                icon = {},
            ) { Text(if (count > 0) "${mode.title} $count" else mode.title, maxLines = 1) }
        }
    }
}

/**
 * `● work-mac`: green when connected, a spinner while connecting, grey when off. It opens
 * Settings. While the demo runs it holds Leave Demo, since a demo user starts on the list.
 */
@Composable
private fun StatusRow(model: AppModel) {
    val host = model.currentHost ?: model.hostList.hosts.firstOrNull { it.id == model.hostList.lastUsed } ?: return
    Row(
        Modifier.fillMaxWidth().clickable { model.showSettings = true }.padding(horizontal = 16.dp, vertical = 4.dp).testTag("status-row"),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        val busy = host.id == model.currentHostId && (model.connecting || model.addingKey)
        if (busy) {
            CircularProgressIndicator(Modifier.size(10.dp), strokeWidth = 1.5.dp)
        } else {
            val on = host.id == model.currentHostId && model.isConnected
            Box(
                Modifier.size(8.dp).background(if (on) Theme.connected else Color.Gray, CircleShape)
                    .semantics { contentDescription = if (on) "Connected" else "Not connected" },
            )
        }
        Text(
            host.title,
            Modifier.weight(1f),
            style = MaterialTheme.typography.labelMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
        if (model.canLeaveDemo) TextButton(model::leaveDemo, Modifier.testTag("leave-demo")) { Text("Leave Demo") }
    }
}

/** No connection: connect to the last host, or on first launch add one or try the demo. */
@Composable
private fun NotConnected(model: AppModel, onEdit: (HostDraft) -> Unit, modifier: Modifier) {
    Column(
        modifier.padding(24.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        val hosts = model.hostList
        if (hosts.hosts.isEmpty()) {
            Text("Add a Host", style = MaterialTheme.typography.titleMedium)
            Text(
                "Muxa connects over SSH to the machine where your tmux sessions run.",
                style = MaterialTheme.typography.bodySmall,
                textAlign = TextAlign.Center,
            )
            Button({ onEdit(HostDraft.new()) }, Modifier.testTag("add-host")) { Text("Add Host") }
            OutlinedButton(model::tryDemo, Modifier.testTag("try-demo")) { Text("Try the Demo") }
            return@Column
        }
        if (model.connecting || model.addingKey) return@Column
        val last = model.currentHost ?: hosts.hosts.firstOrNull { it.id == hosts.lastUsed } ?: hosts.hosts.first()
        Button({ model.connect(last.id) }, Modifier.testTag("connect-last")) { Text("Connect to ${last.title}") }
        OutlinedButton({ model.showSettings = true }, Modifier.testTag("open-hosts")) { Text("Hosts…") }
    }
}

/** A list section's header, with optional buttons at its end. */
@Composable
internal fun SectionHeader(text: String, trailing: @Composable () -> Unit = {}) {
    Row(Modifier.fillMaxWidth().padding(start = 16.dp, end = 4.dp, top = 16.dp), verticalAlignment = Alignment.CenterVertically) {
        Text(text, style = MaterialTheme.typography.labelLarge, color = MaterialTheme.colorScheme.onSurfaceVariant, modifier = Modifier.weight(1f))
        trailing()
    }
}

/** A row that opens something, for the lists' first rows (Routines). */
@Composable
internal fun SidebarRow(title: String, icon: androidx.compose.ui.graphics.vector.ImageVector, tag: String, current: Boolean, trailing: String? = null, onClick: () -> Unit) {
    Row(
        Modifier.fillMaxWidth()
            .background(if (current) MaterialTheme.colorScheme.secondaryContainer else MaterialTheme.colorScheme.surface)
            .clickable(onClick = onClick).padding(horizontal = 16.dp, vertical = 12.dp).testTag(tag),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(icon, null, tint = MaterialTheme.colorScheme.primary)
        Text(title, Modifier.padding(start = 12.dp).weight(1f))
        trailing?.let { Text(it, color = MaterialTheme.colorScheme.onSurfaceVariant) }
        Icon(Icons.chevron, null, tint = MaterialTheme.colorScheme.onSurfaceVariant)
    }
}
