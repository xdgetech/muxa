package tech.xdge.muxa.app.next

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import tech.xdge.muxa.app.ui.Icons
import tech.xdge.muxa.app.ui.LinkButton
import tech.xdge.muxa.app.ui.PrimaryButton
import tech.xdge.muxa.app.ui.Theme
import tech.xdge.muxa.v2.LoopChecks
import tech.xdge.muxa.v2.Patch
import tech.xdge.muxa.v2.SettingsOp
import tech.xdge.muxa.v2.SettingsWorkspace
import tech.xdge.muxa.v2.SubscriptionUsage

/** A Workspace's folders, agent command, Task Loop checks and Routines gate. Each saves on its own. */
@Composable
fun WorkspaceGeneral(editor: SettingsEditor, ws: SettingsWorkspace, plans: List<SubscriptionUsage>) {
    val save = rememberInlineSave(editor)
    val on = !editor.readOnly
    SettingsHeader("Folders")
    SettingsCard {
        Column(Modifier.padding(14.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            SavedField(ws.root, "Root folder", "settings.root", on) { save.run(listOf(SettingsOp.WorkspaceUpdate(ws.name, root = it.trim()))) }
            Lines(ws.repos, "Repos", "settings.repo", on, "app") { save.run(listOf(SettingsOp.WorkspaceUpdate(ws.name, repos = it))) }
            SavedField(ws.parkAfter.orEmpty(), "Park idle Tasks after", "settings.park", on, "Never: 30m, 12h or 2d") {
                val patch = it.trim().ifEmpty { null }?.let { p -> Patch.Set(p) } ?: Patch.Clear
                save.run(listOf(SettingsOp.WorkspaceUpdate(ws.name, parkAfter = patch)))
            }
        }
    }
    SettingsHeader("Agent")
    SettingsCard {
        Column(Modifier.padding(14.dp)) {
            SavedField(ws.agent.command.orEmpty(), "Agent command", "settings.agent", on, "The host's default") {
                val patch = it.trim().ifEmpty { null }?.let { c -> Patch.Set(c) } ?: Patch.Clear
                save.run(listOf(SettingsOp.WorkspaceUpdate(ws.name, agentCommand = patch)))
            }
        }
    }
    WorkspaceLogins(editor, ws, plans) { save.run(it) }
    SettingsHeader("Task Loop checks")
    SettingsCard { LoopChecksEditor(ws, on) { save.run(listOf(SettingsOp.SetLoopChecks(ws.name, it))) } }
    SettingsHeader("Routines")
    SettingsCard {
        Column(Modifier.padding(horizontal = 14.dp, vertical = 6.dp)) {
            SwitchField("Routines allowed", "settings.routines.gate", ws.routinesEnabled, on) {
                save.run(listOf(SettingsOp.RoutinesGate(ws.name, it, ws.directWrites)))
            }
        }
    }
    save.Error()
}

/**
 * The subscriptions its agents use, in order (ADR 0091): each agent takes the first of its kind.
 * A login its env names is adopted from here.
 */
@Composable
private fun WorkspaceLogins(editor: SettingsEditor, ws: SettingsWorkspace, plans: List<SubscriptionUsage>, save: (List<SettingsOp>) -> Unit) {
    val on = !editor.readOnly
    var names by remember(ws.subscriptions) { mutableStateOf(ws.subscriptions) }
    val envLogins = editor.settings?.foundLogins.orEmpty().filter { it.workspace == ws.name }
    SettingsHeader("Subscriptions")
    SettingsCard {
        Column(Modifier.padding(14.dp).testTag("settings.subs"), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            SubscriptionOrder(names, editor.settings?.subscriptionNames().orEmpty(), plans, "settings.subs", on) { names = it }
            if (names != ws.subscriptions) {
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    PrimaryButton("Save", { save(listOf(SettingsOp.WorkspaceUpdate(ws.name, subscriptions = names))) },
                        Modifier.testTag("settings.subs.save"), enabled = on)
                    LinkButton("Undo", { names = ws.subscriptions })
                }
            }
            for (f in envLogins) {
                LinkButton("Make its env login a subscription", { save(listOf(Found.op(f))) }, Modifier.testTag("settings.subs.adopt"), enabled = on)
            }
        }
    }
    SettingsNote(
        if (envLogins.isEmpty()) "Each agent takes the first subscription of its kind. Empty: the agent's default login."
        else "Its env names a login. As a subscription, it shows its usage and the Workspace can list a second one.",
    )
}

/** A text the host keeps, with Save once it differs. */
@Composable
private fun SavedField(value: String, label: String, tag: String, enabled: Boolean, placeholder: String? = null, save: (String) -> Unit) {
    var text by remember(value) { mutableStateOf(value) }
    Field(text, { text = it }, label, tag, enabled = enabled, mono = true, placeholder = placeholder)
    if (text != value) {
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            PrimaryButton("Save", { save(text) }, Modifier.testTag("$tag.save"), enabled = enabled)
            LinkButton("Undo", { text = value })
        }
    }
}

/** A list of lines, each removable, and a field to add one. [problem] says why a line cannot be added, or null. */
@Composable
fun Lines(
    values: List<String>,
    label: String,
    tag: String,
    enabled: Boolean,
    placeholder: String,
    problem: (String) -> String? = { null },
    save: (List<String>) -> Unit,
) {
    var adding by remember(values) { mutableStateOf("") }
    val why = adding.trim().takeIf { it.isNotEmpty() }?.let(problem)
    Text(label, fontSize = 13.sp)
    for (v in values) {
        Row(Modifier.fillMaxWidth().testTag("$tag.$v"), verticalAlignment = Alignment.CenterVertically) {
            Text(v, Modifier.weight(1f), fontFamily = FontFamily.Monospace, fontSize = 14.sp)
            IconButton({ save(values - v) }, Modifier.testTag("$tag.$v.remove"), enabled = enabled) { Icon(Icons.close, "Remove $v") }
        }
    }
    Row(verticalAlignment = Alignment.CenterVertically) {
        Column(Modifier.weight(1f)) { Field(adding, { adding = it }, "Add", "$tag.new", enabled, mono = true, placeholder = placeholder) }
        IconButton(
            { save(values + adding.trim()) }, Modifier.testTag("$tag.add"),
            enabled = enabled && adding.isNotBlank() && adding.trim() !in values && why == null,
        ) {
            Icon(Icons.add, "Add")
        }
    }
    why?.let { Text(it, Modifier.testTag("$tag.problem"), color = Theme.failure, fontSize = 13.sp) }
}

private enum class LoopMode { OFF, ANY, LIST }

/** Off: no Task Loops. Any: any check. List: only these checks. */
@Composable
private fun LoopChecksEditor(ws: SettingsWorkspace, enabled: Boolean, save: (LoopChecks?) -> Unit) {
    val checks = ws.loop?.checks
    val saved = when (checks) {
        null -> LoopMode.OFF
        LoopChecks.Any -> LoopMode.ANY
        is LoopChecks.List -> LoopMode.LIST
    }
    // List shows its field before the first check is saved.
    var mode by remember(saved) { mutableStateOf(saved) }
    Column(Modifier.padding(14.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Choice(LoopMode.entries, listOf("Off", "Any check", "Only these"), mode, "settings.loop", enabled) {
            mode = it
            when (it) {
                LoopMode.OFF -> if (saved != LoopMode.OFF) save(null)
                LoopMode.ANY -> if (saved != LoopMode.ANY) save(LoopChecks.Any)
                LoopMode.LIST -> {}
            }
        }
        if (mode == LoopMode.LIST) {
            val list = (checks as? LoopChecks.List)?.commands.orEmpty()
            Lines(list, "Checks", "settings.check", enabled, "make test") { save(if (it.isEmpty()) null else LoopChecks.List(it)) }
        }
    }
}
