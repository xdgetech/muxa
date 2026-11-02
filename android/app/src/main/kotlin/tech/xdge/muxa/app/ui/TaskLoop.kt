package tech.xdge.muxa.app.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Checkbox
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ExposedDropdownMenuBox
import androidx.compose.material3.ExposedDropdownMenuDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ExposedDropdownMenuAnchorType
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.unit.dp
import tech.xdge.muxa.app.AppModel
import tech.xdge.muxa.app.ReaderTab
import tech.xdge.muxa.protocol.TaskLoop
import tech.xdge.muxa.protocol.TaskRecord
import tech.xdge.muxa.protocol.WorkspaceChecks
import tech.xdge.muxa.session.InboxItem
import tech.xdge.muxa.session.LOOP_NEEDS_DAEMON
import tech.xdge.muxa.session.LoopDraft
import tech.xdge.muxa.session.isActive
import tech.xdge.muxa.session.isKnownState
import tech.xdge.muxa.session.lastCheckLine
import tech.xdge.muxa.session.loopAttempts
import tech.xdge.muxa.session.loopBadge
import tech.xdge.muxa.session.loopExplanation
import tech.xdge.muxa.session.loopStateTitle

/*
 * The Task Loop in the Tasks screen (ADR 0019): the detail's Loop section, the Start loop
 * sheet, and the Inbox row's badge. Labels match the iOS app.
 */

/** The Task's loop as its record says, and the buttons that start or stop it. */
@OptIn(androidx.compose.foundation.layout.ExperimentalLayoutApi::class)
@Composable
fun LoopSection(model: AppModel, record: TaskRecord, item: InboxItem) {
    val loop = record.loop
    val checks = model.checksOf(record.workspace)
    Column(Modifier.testTag("task-loop"), verticalArrangement = Arrangement.spacedBy(6.dp)) {
        Text("Loop", style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
        if (loop != null) LoopFacts(loop) else Text("This Task has no loop.")
        if (!model.canLoop) {
            Text(LOOP_NEEDS_DAEMON, style = MaterialTheme.typography.bodySmall, modifier = Modifier.testTag("loop-needs-daemon"))
        } else if (checks == null && loop?.isActive != true) {
            Text(LoopDraft.NO_CHECKS, style = MaterialTheme.typography.bodySmall, modifier = Modifier.testTag("loop-no-checks"))
        }
        FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            // A state this app does not know may still be running: offer both.
            val unknown = loop != null && !loop.isKnownState
            if (model.canLoop && checks != null && (loop?.isActive != true || unknown)) {
                OutlinedButton({ model.startLoopSheet(record) }, Modifier.testTag("loop-start")) { Text("Start loop") }
            }
            if (model.canLoop && loop != null && (loop.isActive || unknown)) {
                OutlinedButton({ model.stopLoop(record) }, Modifier.testTag("loop-stop")) { Text("Stop loop") }
            }
            // The daemon writes CHECK.md after each check; the Reader reads it from the Task's session.
            if (loop?.lastCheck != null && model.state.canReadDocs && item.session != null) {
                OutlinedButton({ model.readTask(record, ReaderTab.DOCS, AppModel.CHECK_DOC) }, Modifier.testTag("loop-open-log")) {
                    Text("Open check log")
                }
            }
        }
    }
}

@Composable
private fun LoopFacts(loop: TaskLoop) {
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        Text(loopStateTitle(loop.state), fontWeight = FontWeight.SemiBold, modifier = Modifier.testTag("loop-state"))
        Text("Attempts ${loopAttempts(loop)}", style = MaterialTheme.typography.bodySmall, modifier = Modifier.testTag("loop-attempts"))
    }
    Text(
        loopExplanation(loop),
        color = if (loop.needsUser) Theme.failure else MaterialTheme.colorScheme.onSurface,
        modifier = Modifier.testTag("loop-reason"),
    )
    Column {
        Text("Check", style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
        SelectionContainer { Text(loop.check, fontFamily = FontFamily.Monospace, modifier = Modifier.testTag("loop-check-command")) }
    }
    loop.lastCheck?.let { check ->
        Column {
            Text("Last check", style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
            Text(lastCheckLine(check, System.currentTimeMillis() / 1000), modifier = Modifier.testTag("loop-last-check"))
        }
    }
}

/** The Inbox row's loop badge; in the failure color when the loop needs the user. */
@Composable
fun LoopBadge(record: TaskRecord) {
    val loop = record.loop ?: return
    Text(
        loopBadge(loop),
        style = MaterialTheme.typography.labelSmall,
        color = if (loop.needsUser) Theme.failure else MaterialTheme.colorScheme.onSurfaceVariant,
        modifier = Modifier.testTag("loop-badge-${record.id}"),
    )
}

/** Asks for the check and max attempts, then starts the loop. */
@Composable
fun StartLoopDialog(model: AppModel) {
    val (id, initial) = model.startingLoop ?: return
    val record = model.state.tasks?.record(id) ?: return
    var draft by remember(id) { mutableStateOf(initial) }
    var reset by remember(id) { mutableStateOf(false) }
    AlertDialog(
        onDismissRequest = { model.startingLoop = null },
        title = { Text("Start the loop") },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(LOOP_EXPLAINED, style = MaterialTheme.typography.bodySmall)
                LoopFields(draft) { draft = it }
                if ((record.loop?.attempts ?: 0) > 0) {
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        Checkbox(reset, { reset = it }, Modifier.testTag("loop-reset"))
                        Text("Count attempts from zero")
                    }
                }
                draft.problem?.takeIf { draft.checks != WorkspaceChecks.Any || draft.check.isNotEmpty() }?.let {
                    Text(it, color = Theme.failure, modifier = Modifier.testTag("loop-problem"))
                }
            }
        },
        confirmButton = {
            TextButton({ model.startLoop(record, draft, reset) }, enabled = draft.problem == null, modifier = Modifier.testTag("loop-start-confirm")) {
                Text("Start")
            }
        },
        dismissButton = { TextButton({ model.startingLoop = null }) { Text("Cancel") } },
    )
}

@Composable
fun LoopErrorDialog(model: AppModel) {
    val error = model.loopError ?: return
    AlertDialog(
        onDismissRequest = { model.loopError = null },
        title = { Text("Can't Change the Loop") },
        text = { Text(error, Modifier.testTag("loop-error-message")) },
        confirmButton = { TextButton({ model.loopError = null }) { Text("OK") } },
    )
}

/** The check, from the workspace's list or typed for `"any"`, and the max attempts. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun LoopFields(draft: LoopDraft, onChange: (LoopDraft) -> Unit) {
    when (val checks = draft.checks) {
        null -> Text(LoopDraft.NO_CHECKS, style = MaterialTheme.typography.bodySmall)
        WorkspaceChecks.Any -> OutlinedTextField(
            draft.check, { onChange(draft.copy(check = it)) }, Modifier.fillMaxWidth().testTag("loop-check"),
            label = { Text("Check command") }, placeholder = { Text("make test") }, singleLine = true,
            textStyle = TextStyle(fontFamily = FontFamily.Monospace),
            keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.None, autoCorrectEnabled = false),
        )
        is WorkspaceChecks.Only -> Picker("Check", draft.check, checks.commands, "loop-check", "check-option", mono = true) {
            onChange(draft.copy(check = it))
        }
    }
    if (draft.checks != null) {
        Picker("Max attempts", draft.maxAttempts.toString(), LoopDraft.ATTEMPTS.map { it.toString() }, "loop-max-attempts", "attempts") {
            onChange(draft.copy(maxAttempts = it.toInt()))
        }
    }
}

/** A read-only field that picks one of [options]; option i is tagged "[optionTag]-i". */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun Picker(label: String, value: String, options: List<String>, tag: String, optionTag: String, mono: Boolean = false, pick: (String) -> Unit) {
    var open by remember { mutableStateOf(false) }
    val font = if (mono) FontFamily.Monospace else null
    ExposedDropdownMenuBox(open, { open = it }) {
        OutlinedTextField(
            value, {},
            Modifier.fillMaxWidth().menuAnchor(ExposedDropdownMenuAnchorType.PrimaryNotEditable).testTag(tag),
            readOnly = true, singleLine = true, label = { Text(label) },
            textStyle = TextStyle(fontFamily = font),
            trailingIcon = { ExposedDropdownMenuDefaults.TrailingIcon(open) },
        )
        ExposedDropdownMenu(open, { open = false }) {
            options.forEachIndexed { i, option ->
                DropdownMenuItem({ Text(option, fontFamily = font) }, {
                    pick(option)
                    open = false
                }, modifier = Modifier.testTag("$optionTag-$i"))
            }
        }
    }
}

/** The New Task sheet's switch, and the loop's fields when it is on. */
@Composable
fun NewTaskLoop(model: AppModel, loop: LoopDraft?, checks: WorkspaceChecks?, onChange: (LoopDraft?) -> Unit) {
    if (!model.canLoop) {
        Text(LOOP_NEEDS_DAEMON, style = MaterialTheme.typography.bodySmall)
        return
    }
    Row(verticalAlignment = Alignment.CenterVertically) {
        Text("Run a check after each turn", Modifier.weight(1f))
        Switch(
            loop != null, { onChange(if (it) LoopDraft(checks) else null) },
            Modifier.testTag("task-with-loop"), enabled = checks != null,
        )
    }
    if (checks == null) {
        Text(LoopDraft.NO_CHECKS, style = MaterialTheme.typography.bodySmall, modifier = Modifier.padding(bottom = 4.dp))
    } else if (loop != null) {
        LoopFields(loop) { onChange(it) }
    }
}

private const val LOOP_EXPLAINED =
    "The host runs the check after each of the agent's turns. When it fails, the agent is asked to fix it, until the check passes or the attempts run out."
