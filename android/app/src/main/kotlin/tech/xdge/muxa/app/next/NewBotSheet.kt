package tech.xdge.muxa.app.next

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.input.TextFieldValue
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.launch
import tech.xdge.muxa.app.ui.LinkButton
import tech.xdge.muxa.app.ui.Palette
import tech.xdge.muxa.app.ui.PrimaryButton
import tech.xdge.muxa.app.ui.SecondaryButton
import tech.xdge.muxa.app.ui.Theme
import tech.xdge.muxa.v2.BotSpec
import tech.xdge.muxa.v2.SettingsOp

/**
 * New Bot (ADR 0086 decision 3): a name, one sentence and the Workspace, whose access it runs with
 * (ADR 0087). Create adds the Bot and opens its first chat on its page; its Routines come from that
 * chat as proposals.
 */
@Composable
fun NewBotSheet(model: NextModel, nav: Nav, editor: SettingsEditor, close: () -> Unit) {
    var name by remember { mutableStateOf(TextFieldValue("")) }
    var sentence by remember { mutableStateOf("") }
    val workspaces = editor.settings?.workspaces.orEmpty().map { it.name }
    var workspace by remember { mutableStateOf(nav.bots.pick?.workspace) }
    var error by remember { mutableStateOf<String?>(null) }
    var creating by remember { mutableStateOf(false) }
    val scope = rememberCoroutineScope()
    LaunchedEffect(workspaces) { if (workspace !in workspaces) workspace = workspaces.firstOrNull() }
    val picked = workspace
    val typed = BotDraft.typedName(name.text.trim())
    val taken = editor.settings?.workspaces?.firstOrNull { it.name == picked }?.bots?.any { it.name == typed } == true
    val (hint, problem) = newBotHint(editor, workspaces.isEmpty(), typed, taken, picked)
    val valid = BotDraft.isValidName(typed) && !taken && picked != null && !editor.readOnly

    // Every value it uses is a parameter: Compose remembers the button's lambda by what it captures.
    fun create(ws: String, spec: BotSpec) = scope.launch {
        creating = true
        error = editor.save(listOf(SettingsOp.BotAdd(ws, spec)))
        if (error == null) {
            val goal = spec.description?.let { "You are ${spec.name}: $it. Ask me what to watch, then propose your Routines." }
            val t = model.startTask(TaskDraft.talk(ws, spec.name).copy(goal = goal))
            if (t == null) {
                error = "${spec.name} was added, but its chat did not start: ${model.requestError ?: "unknown error"}"
            } else {
                close()
                nav.open(t)
            }
        }
        creating = false
    }

    AlertDialog(
        onDismissRequest = close,
        modifier = Modifier.testTag("newbot.sheet"),
        title = { Text("New Bot") },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text("It starts with a chat. Tell it what to watch; it proposes Routines you apply.", style = MaterialTheme.typography.bodySmall)
                BotNameField(name, { name = it }, "newbot.name", help = null, placeholder = "deps")
                Field(sentence, { sentence = it }, "What it does", "newbot.description", placeholder = "Keeps the app's dependencies up to date")
                WorkspacePicker(workspaces, picked) { workspace = it }
                Text(hint, Modifier.testTag("newbot.hint"), color = if (problem) Theme.failure else Palette.textTertiary,
                    style = MaterialTheme.typography.bodySmall)
                picked?.let { Text("It runs with $it's access: its connections and MCP servers.", style = MaterialTheme.typography.bodySmall) }
                error?.let { Text(it, Modifier.testTag("newbot.error"), color = Theme.failure) }
            }
        },
        confirmButton = {
            PrimaryButton("Create and Chat", {
                if (picked != null) create(picked, BotSpec(typed, sentence.trim().ifEmpty { null }))
            }, Modifier.testTag("newbot.create"), enabled = valid && !creating)
        },
        dismissButton = { LinkButton("Cancel", close) },
    )
}

/** Why Create is disabled, first reason first; else the naming rule. The second value says it is a problem. */
private fun newBotHint(editor: SettingsEditor, noWorkspaces: Boolean, name: String, taken: Boolean, workspace: String?): Pair<String, Boolean> =
    when {
        editor.readOnly -> "The host's settings are read only, so no Bot can be added from here." to true
        noWorkspaces && editor.loadError != null -> "The settings could not be read: ${editor.loadError}" to true
        noWorkspaces -> "Reading the host's Workspaces…" to false
        taken -> "$workspace has a Bot named $name." to true
        name.isNotEmpty() && !BotDraft.isValidName(name) -> "A name is 1 to 64 lowercase letters, digits, - and _." to true
        else -> "Lowercase letters, digits, - and _." to false
    }

@Composable
private fun WorkspacePicker(workspaces: List<String>, picked: String?, pick: (String) -> Unit) {
    var open by remember { mutableStateOf(false) }
    Box {
        SecondaryButton("Workspace: ${picked ?: "none"}", { open = true }, Modifier.testTag("newbot.workspace"))
        DropdownMenu(open, { open = false }) {
            for (w in workspaces) {
                DropdownMenuItem({ Text(w) }, {
                    open = false
                    pick(w)
                }, Modifier.testTag("newbot.workspace.$w"))
            }
        }
    }
}
