package tech.xdge.muxa.app.ui

import androidx.compose.runtime.getValue
import androidx.compose.runtime.setValue
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.testTagsAsResourceId
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import tech.xdge.muxa.app.AppModel
import tech.xdge.muxa.app.BuildConfig
import tech.xdge.muxa.session.Host

/**
 * Settings (ADR 0035): the hosts, with connect, add, edit, delete, reorder and Disconnect;
 * the demo; the version. It covers the window like the Reader. [onEdit] opens the host editor.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun SettingsScreen(model: AppModel, onEdit: (HostDraft) -> Unit) {
    val close = { model.showSettings = false; model.refocusPane() }
    Dialog(onDismissRequest = close, properties = DialogProperties(usePlatformDefaultWidth = false, decorFitsSystemWindows = false)) {
        DialogStatusBar()
        // A dialog is its own window: it needs the tag setting the main window has (Root).
        Surface(Modifier.fillMaxSize().semantics { testTagsAsResourceId = true }, color = MaterialTheme.colorScheme.background) {
            Column(Modifier.fillMaxSize().navigationBarsPadding().testTag("settings")) {
                TopAppBar(
                    title = { Text("Settings") },
                    navigationIcon = { IconButton(close, Modifier.testTag("settings-done")) { Icon(Icons.close, "Done") } },
                    actions = { IconButton({ onEdit(HostDraft.new()) }, Modifier.testTag("settings-add-host")) { Icon(Icons.add, "Add Host") } },
                )
                SettingsList(model, onEdit, close)
            }
        }
    }
}

@Composable
private fun SettingsList(model: AppModel, onEdit: (HostDraft) -> Unit, close: () -> Unit) {
    val hosts = model.hostList.hosts
    LazyColumn(Modifier.fillMaxWidth()) {
        item(key = "hosts") { SectionHeader("Hosts") }
        if (hosts.isEmpty()) {
            item(key = "no-hosts") {
                Text("No hosts yet. Add the machine where your tmux sessions run.", Modifier.padding(16.dp), style = MaterialTheme.typography.bodySmall)
            }
        }
        itemsIndexed(hosts, key = { _, h -> h.id }) { index, host ->
            HostRow(model, host, index, hosts.size, onEdit) {
                model.connect(host.id)
                close()
            }
        }
        if (model.isConnected) {
            item(key = "disconnect") {
                SettingsAction("Disconnect", Icons.disconnect, "settings-disconnect", Theme.failure) { model.disconnect() }
            }
        }
        item(key = "demo") { SectionHeader("Demo") }
        item(key = "demo-action") {
            if (model.canLeaveDemo) {
                SettingsAction("Leave Demo", Icons.leaveDemo, "settings-leave-demo") { model.leaveDemo() }
            } else {
                SettingsAction("Try the Demo", Icons.demo, "settings-try-demo") { model.tryDemo() }
            }
        }
        item(key = "about") { SectionHeader("About") }
        item(key = "version") {
            Text("Muxa ${BuildConfig.VERSION_NAME}", Modifier.padding(16.dp).testTag("settings-version"), style = MaterialTheme.typography.bodyMedium)
        }
    }
}

@Composable
private fun SettingsAction(title: String, icon: ImageVector, tag: String, tint: androidx.compose.ui.graphics.Color? = null, onClick: () -> Unit) {
    Row(
        Modifier.fillMaxWidth().clickable(onClick = onClick).padding(horizontal = 16.dp, vertical = 12.dp).testTag(tag),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(icon, null, tint = tint ?: MaterialTheme.colorScheme.primary)
        Text(title, Modifier.padding(start = 12.dp), color = tint ?: MaterialTheme.colorScheme.primary)
    }
}

/** A tap connects; the menu edits, deletes and moves the host in the list. */
@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun HostRow(model: AppModel, host: Host, index: Int, count: Int, onEdit: (HostDraft) -> Unit, connect: () -> Unit) {
    var menu by remember { mutableStateOf(false) }
    val current = host.id == model.currentHostId
    Box {
        Row(
            Modifier.fillMaxWidth()
                .combinedClickable(onClick = connect, onLongClick = { menu = true })
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
            IconButton({ menu = true }, Modifier.testTag("host-actions-${host.title}")) { Icon(Icons.more, "Host Actions") }
        }
        DropdownMenu(menu, { menu = false }) {
            DropdownMenuItem({ Text("Edit") }, { menu = false; onEdit(HostDraft.of(host)) }, leadingIcon = { Icon(Icons.edit, null) })
            if (index > 0) {
                DropdownMenuItem({ Text("Move Up") }, { menu = false; model.hosts.move(index, index - 1) }, leadingIcon = { Icon(Icons.moveUp, null) })
            }
            if (index < count - 1) {
                DropdownMenuItem({ Text("Move Down") }, { menu = false; model.hosts.move(index, index + 1) }, leadingIcon = { Icon(Icons.moveDown, null) })
            }
            DropdownMenuItem(
                { Text("Delete", color = Theme.failure) },
                { menu = false; model.hosts.delete(host.id) },
                leadingIcon = { Icon(Icons.delete, null, tint = Theme.failure) },
                modifier = Modifier.testTag("host-delete"),
            )
        }
    }
}
