package tech.xdge.muxa.app.next

import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import tech.xdge.muxa.v2.SecretState
import tech.xdge.muxa.v2.SettingsConnection
import tech.xdge.muxa.v2.SettingsOp
import tech.xdge.muxa.v2.SettingsWorkspace
import tech.xdge.muxa.v2.Workspace

/** A Workspace's connections: a token in the host's Keychain that goes into each agent's environment. */
@Composable
fun ConnectionSettings(editor: SettingsEditor, ws: SettingsWorkspace, usage: Workspace?, model: NextModel) {
    var editing by remember { mutableStateOf<ConnectionDraft?>(null) }
    var secretState by remember { mutableStateOf(SecretState.MISSING) }
    WorkspaceToolsNote(ws)
    SettingsHeader("Connections")
    SettingsCard {
        if (ws.connections.isEmpty()) SettingsNote("No connections.")
        for (c in ws.connections) {
            val used = usage?.connections?.firstOrNull { it.name == c.name }
            EntryRow("settings.connection.${ws.name}.${c.name}", !editor.readOnly, {
                editing = ConnectionDraft.of(c)
                secretState = c.secret
            }, badge = null) {
                EntryTitle(c.name) {
                    AccessChip(c.access)
                    SecretChip(c.secret, Modifier.testTag("settings.connection.${ws.name}.${c.name}.secret"))
                }
                EntryDetail(listOfNotNull(c.env, c.account).joinToString(" · "), mono = true)
                UsageLines(used?.usedBy, used?.lastUsed, model)
            }
            CardDivider()
        }
        ActionRow("+ Add connection", "settings.connection.add", !editor.readOnly) {
            editing = ConnectionDraft(isNew = true)
            secretState = SecretState.MISSING
        }
    }
    editing?.let { ConnectionDialog(editor, ws.name, it, secretState) { editing = null } }
}

@Composable
private fun ConnectionDialog(editor: SettingsEditor, workspace: String, initial: ConnectionDraft, secret: SecretState, close: () -> Unit) {
    var draft by remember { mutableStateOf(initial) }
    suspend fun save(): String? {
        val op = draft.op(workspace)
        // The secret leaves the form with the op, whatever the host answers.
        draft = draft.copy(secret = "")
        return editor.save(listOf(op))
    }
    FormDialog(
        if (draft.isNew) "New Connection" else draft.name, "settings.connection.form", draft.isValid, { save() }, close,
        remove = if (draft.isNew) null else ({ editor.save(listOf(SettingsOp.ConnectionRemove(workspace, draft.name))) }),
        removeTitle = "Remove Connection",
    ) {
        Field(draft.name, { draft = draft.copy(name = it) }, "Name", "connection.name", enabled = draft.isNew, placeholder = "github")
        Field(draft.env, { draft = draft.copy(env = it) }, "Variable", "connection.env", mono = true, placeholder = "GITHUB_TOKEN")
        Field(draft.account, { draft = draft.copy(account = it) }, "Account", "connection.account", placeholder = "Optional label")
        Text("Access", style = MaterialTheme.typography.titleSmall)
        Choice(listOf(null, "read", "write"), listOf("Not said", "Read", "Write"), draft.access, "connection.access") {
            draft = draft.copy(access = it)
        }
        SecretField(draft.secret, { draft = draft.copy(secret = it) }, "Secret", "connection.secret", isSet = secret == SecretState.SET)
        if (!draft.isNew) {
            Text("In the Keychain: ${secret.wire}", style = MaterialTheme.typography.bodySmall)
        }
        Text("The secret goes to the host's Keychain. muxa never shows it again.", style = MaterialTheme.typography.bodySmall)
    }
}

/** The connections by name, for an MCP server's choices. */
fun SettingsWorkspace.connectionNames(): List<String> = connections.map(SettingsConnection::name)
