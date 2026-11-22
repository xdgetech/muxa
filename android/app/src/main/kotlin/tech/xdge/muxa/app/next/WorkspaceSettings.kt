package tech.xdge.muxa.app.next

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.launch
import tech.xdge.muxa.app.ui.Icons
import tech.xdge.muxa.app.ui.Palette
import tech.xdge.muxa.app.ui.Theme
import tech.xdge.muxa.v2.SettingsOp
import tech.xdge.muxa.v2.SettingsWorkspace
import tech.xdge.muxa.v2.Workspace

private enum class WorkspaceTab(val title: String) { GENERAL("General"), CONNECTIONS("Connections"), MCP("MCP"), ROUTINES("Routines") }

/** One Workspace's settings in tabs. [usage] is its entry of `workspaces`: who uses what, and when. */
@Composable
fun WorkspaceSettings(editor: SettingsEditor, ws: SettingsWorkspace, usage: Workspace?, model: NextModel, back: () -> Unit) {
    var tab by rememberSaveable { mutableStateOf(WorkspaceTab.GENERAL) }
    Column(Modifier.testTag("settings.detail.${ws.name}")) {
        Row(Modifier.fillMaxWidth().padding(start = 4.dp, end = 14.dp, top = 4.dp), verticalAlignment = Alignment.CenterVertically) {
            IconButton(back, Modifier.testTag("settings.back")) { Icon(Icons.back, "Settings") }
            Column(Modifier.weight(1f)) {
                Text(ws.name, fontSize = 20.sp, fontWeight = FontWeight.SemiBold)
                Text(ws.root, color = Palette.textTertiary, fontSize = 12.5.sp, fontFamily = FontFamily.Monospace)
            }
        }
        Column(Modifier.padding(horizontal = 12.dp, vertical = 6.dp)) {
            Choice(WorkspaceTab.entries, WorkspaceTab.entries.map { it.title }, tab, "settings.tab") { tab = it }
        }
        when (tab) {
            WorkspaceTab.GENERAL -> {
                WorkspaceGeneral(editor, ws)
                EnvSettings(editor, ws)
                RemoveWorkspace(editor, ws, back)
            }
            WorkspaceTab.CONNECTIONS -> ConnectionSettings(editor, ws, usage, model)
            WorkspaceTab.MCP -> McpSettings(editor, ws, usage, model)
            WorkspaceTab.ROUTINES -> RoutineSettings(editor, ws)
        }
    }
}

/** Every agent of the Workspace gets all of them (ADR 0075). */
@Composable
fun WorkspaceToolsNote(ws: SettingsWorkspace) {
    SettingsInfo(
        "Every agent of ${ws.name}, its Tasks and its Routines, gets all its connections and MCP servers. " +
            "Listing an MCP server makes MCP strict: its agents get only the servers listed here.",
        "settings.tools.${ws.name}",
    )
}

@Composable
private fun RemoveWorkspace(editor: SettingsEditor, ws: SettingsWorkspace, back: () -> Unit) {
    var asking by remember { mutableStateOf(false) }
    val save = rememberInlineSave(editor)
    SettingsCard {
        ActionRow("Remove Workspace", "settings.workspace.remove", !editor.readOnly, Theme.failure) { asking = true }
    }
    save.Error()
    if (asking) {
        ConfirmDialog(
            "Remove ${ws.name}?",
            "muxa forgets this Workspace and removes its connections' secrets from the Keychain. " +
                "Its folder ${ws.root} and its .muxa/ stay on the host.",
            "Remove",
            {
                asking = false
                save.run(listOf(SettingsOp.WorkspaceRemove(ws.name)), back)
            },
        ) { asking = false }
    }
}

/** Saves a change made in place, such as a switch, and keeps its error until the next one. */
@Stable
class InlineSave(private val scope: CoroutineScope, private val editor: SettingsEditor) {
    var error: String? by mutableStateOf(null)
        private set

    fun run(ops: List<SettingsOp>, saved: () -> Unit = {}) {
        scope.launch {
            error = editor.save(ops)
            if (error == null) saved()
        }
    }

    @Composable
    fun Error() {
        error?.let {
            Text(it, Modifier.padding(horizontal = 18.dp, vertical = 4.dp).testTag("settings.inline.error"), color = Theme.failure, fontSize = 13.sp)
        }
    }
}

@Composable
fun rememberInlineSave(editor: SettingsEditor): InlineSave {
    val scope = rememberCoroutineScope()
    return remember(editor) { InlineSave(scope, editor) }
}
