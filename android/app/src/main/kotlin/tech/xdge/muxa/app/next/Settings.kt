package tech.xdge.muxa.app.next

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import tech.xdge.muxa.app.ui.Icons
import tech.xdge.muxa.app.ui.Palette
import tech.xdge.muxa.v2.Workspace

/** The phone's Settings tab: the host this window is connected to and its settings. */
@Composable
fun SettingsScreen(model: NextModel, nav: Nav) {
    PhoneTabs(nav) {
        LargeTitle("Settings")
        Column(Modifier.weight(1f).verticalScroll(rememberScrollState())) { SettingsContent(model) }
    }
}

/** A tablet's Settings, opened from the host in the sidebar's footer: the phone tab's content as a sheet. */
@Composable
fun SettingsDialog(model: NextModel, close: () -> Unit) {
    Dialog(close, DialogProperties(usePlatformDefaultWidth = false)) {
        Surface(
            Modifier.widthIn(max = SHEET_WIDTH.dp).heightIn(max = SHEET_HEIGHT.dp).testTag("settings.sheet"),
            shape = MaterialTheme.shapes.large,
            color = Palette.surface,
            border = BorderStroke(1.dp, Palette.border),
        ) {
            Column {
                Row(Modifier.fillMaxWidth().padding(start = 18.dp, end = 8.dp, top = 8.dp), verticalAlignment = Alignment.CenterVertically) {
                    Text("Settings", Modifier.weight(1f), fontSize = 22.sp, fontWeight = FontWeight.Bold)
                    TextButton(close, Modifier.testTag("settings.close")) { Text("Done") }
                }
                Column(Modifier.weight(1f, fill = false).verticalScroll(rememberScrollState())) { SettingsContent(model) }
            }
        }
    }
}

/** The host, then its settings (ADR 0074): the Workspaces list or one Workspace, and the workers. */
@Composable
private fun SettingsContent(model: NextModel) {
    val editor = remember(model) { SettingsEditor(model.settingsHost) }
    var usage by remember { mutableStateOf<List<Workspace>>(emptyList()) }
    var open by remember { mutableStateOf<String?>(null) }
    LaunchedEffect(editor) { editor.load() }
    // Tasks that start or end change who uses what: ask again when the live terminals change.
    val live = model.terminals.filter { it.live }.map { it.id }
    LaunchedEffect(live) { model.workspaces()?.let { usage = it } }

    Column(Modifier.padding(bottom = 16.dp)) {
        val settings = editor.settings
        val ws = settings?.workspaces?.firstOrNull { it.name == open }
        if (ws == null) {
            HostCard(model)
            SettingsNote("Hosts are added and edited while disconnected.")
        }
        Notices(editor)
        when {
            settings == null && editor.loadError != null -> SettingsNote(editor.loadError.orEmpty())
            settings == null -> Box(Modifier.fillMaxWidth().padding(16.dp), contentAlignment = Alignment.Center) { CircularProgressIndicator() }
            ws != null -> WorkspaceSettings(editor, ws, usage.firstOrNull { it.name == ws.name }, model) { open = null }
            else -> {
                WorkspaceList(editor) { open = it }
                WorkersSection(editor, settings.workspaces.map { it.name })
            }
        }
    }
}

@Composable
private fun Notices(editor: SettingsEditor) {
    val notice = editor.notice
    if (notice != null) SettingsWarning(notice, "settings.notice")
    if (editor.readOnly && notice != SettingsEditor.LOCKED) SettingsWarning(SettingsEditor.LOCKED, "settings.readonly")
    editor.settings?.problem?.let { SettingsWarning("workspaces.toml cannot be used as it is: $it", "settings.problem") }
}

@Composable
private fun HostCard(model: NextModel) {
    SettingsCard {
        Row(Modifier.padding(horizontal = 14.dp, vertical = 12.dp), verticalAlignment = Alignment.CenterVertically) {
            Icon(Icons.host, null, tint = Palette.textSecondary)
            Spacer(Modifier.width(12.dp))
            Column(Modifier.weight(1f)) {
                Text(hostName(model), fontSize = 16.sp, fontWeight = FontWeight.SemiBold)
                Text(hostLine(model).substringAfter(" · "), color = Palette.textTertiary, fontSize = 12.5.sp)
            }
        }
        CardDivider()
        Row(
            Modifier
                .fillMaxWidth()
                .heightIn(min = 48.dp)
                .clickable(role = Role.Button) { model.disconnect() }
                .padding(horizontal = 14.dp)
                .testTag("settings.disconnect"),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(if (model.demo) "Leave the Demo" else "Disconnect", color = Palette.accent)
        }
    }
}

@Composable
private fun WorkspaceList(editor: SettingsEditor, open: (String) -> Unit) {
    var adding by remember { mutableStateOf(false) }
    val list = editor.settings?.workspaces.orEmpty()
    Column(Modifier.testTag("settings.workspaces")) {
        SettingsHeader("Workspaces")
        SettingsInfo(
            "Secrets and env values stay on the host, secrets in its Keychain. muxa never shows them: it only says whether one is set.",
            "settings.secrets",
        )
        SettingsCard {
            if (list.isEmpty()) SettingsNote("This host has no Workspaces yet.", Modifier.padding(vertical = 8.dp))
            for (w in list) {
                val parts = listOf(w.root, count(w.connections.size, "connection"), count(w.mcp.size, "MCP server"))
                NavRow(w.name, parts.joinToString(" · "), "settings.workspace.${w.name}", { open(w.name) })
                CardDivider()
            }
            ActionRow("+ New Workspace", "settings.workspace.add", !editor.readOnly) { adding = true }
        }
    }
    if (adding) NewWorkspaceDialog(editor) { adding = false }
}

@Composable
private fun NewWorkspaceDialog(editor: SettingsEditor, close: () -> Unit) {
    var draft by remember { mutableStateOf(WorkspaceDraft()) }
    FormDialog("New Workspace", "settings.workspace.form", draft.isValid, { editor.save(listOf(draft.op())) }, close) {
        Field(draft.name, { draft = draft.copy(name = it) }, "Name", "workspace.name")
        Field(draft.root, { draft = draft.copy(root = it) }, "Root folder", "workspace.root", mono = true, placeholder = "~/Projects/app")
        Field(
            draft.repos, { draft = draft.copy(repos = it) }, "Repos", "workspace.repos", mono = true, lines = 2,
            help = "One per line, relative to the root. Optional.",
        )
    }
}

fun count(n: Int, noun: String): String = if (n == 1) "1 $noun" else "$n ${noun}s"

private const val SHEET_WIDTH = 640
private const val SHEET_HEIGHT = 760
