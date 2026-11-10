package tech.xdge.muxa.app.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.DatePicker
import androidx.compose.material3.DatePickerDialog
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TimeInput
import androidx.compose.material3.rememberDatePickerState
import androidx.compose.material3.rememberTimePickerState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.launch
import tech.xdge.muxa.app.AppModel
import tech.xdge.muxa.app.applyNotes
import tech.xdge.muxa.app.canPlan
import tech.xdge.muxa.app.notesKey
import tech.xdge.muxa.app.setDue
import tech.xdge.muxa.app.todayRow
import tech.xdge.muxa.protocol.AssistantRequest
import tech.xdge.muxa.protocol.WorkItem
import tech.xdge.muxa.session.DueDraft
import tech.xdge.muxa.session.delegateName
import tech.xdge.muxa.session.proposedNotes
import java.time.Instant
import java.time.LocalTime
import java.time.ZoneOffset

// A Work item's part of the overall assistant (ADR 0049 §1, §5, §6): why Today lists it, the
// delegate's proposed notes, and the user's deadline.

/** "In Today: …" with the plan's reason, when the shown plan lists the item. */
@Composable
internal fun TodayReason(model: AppModel, item: WorkItem) {
    val row = model.todayRow(item) ?: return
    Text("In Today: ${row.why}", Modifier.testTag("work-detail-today"), style = MaterialTheme.typography.bodyMedium)
}

/**
 * The newest Result's proposed notes, in full, with Update notes. Nothing is written until the user
 * taps it; a live file that changed since the run read it is refused, and the user asks again.
 */
@Composable
internal fun NotesSection(model: AppModel, requests: List<AssistantRequest>) {
    if (!model.canPlan) return
    val (request, result) = proposedNotes(requests) ?: return
    val notes = result.notes ?: return
    var error by remember(request.id, result.version) { mutableStateOf<String?>(null) }
    var sending by remember(request.id, result.version) { mutableStateOf(false) }
    val scope = rememberCoroutineScope()
    val applied = notesKey(request, result) in model.appliedNotes
    Column(Modifier.fillMaxWidth().testTag("work-notes"), verticalArrangement = Arrangement.spacedBy(4.dp)) {
        DetailHeader("Notes")
        DetailNote("${delegateName(request.workspace)} proposes these notes for later runs. They replace .muxa/assistant/NOTES.md.")
        SelectionContainer {
            Text(notes.text, Modifier.testTag("work-notes-text"), fontFamily = FontFamily.Monospace, style = MaterialTheme.typography.bodySmall)
        }
        if (applied) {
            Text("Notes updated.", Modifier.testTag("work-notes-applied"), color = Theme.connected)
        } else {
            TextButton({
                sending = true
                scope.launch {
                    error = model.applyNotes(request, result)
                    sending = false
                }
            }, Modifier.testTag("work-notes-apply"), enabled = !sending) { Text("Update notes") }
        }
        error?.let { Text(it, Modifier.testTag("work-notes-error"), color = Theme.failure) }
    }
}

/** Due… opens the deadline sheet; the item's deadline, or tomorrow at 17:00 in this device's zone. */
@Composable
internal fun DueButton(model: AppModel, item: WorkItem, busy: Boolean) {
    var open by remember(item.id) { mutableStateOf(false) }
    TextButton({ open = true }, Modifier.testTag("work-due"), enabled = !busy) { Text(if (item.due == null) "Set Due…" else "Change Due…") }
    if (open) DueDialog(model, item) { open = false }
}

/** A date, a time and an IANA zone. Only the user sets a deadline; a plan's estimate stays in the plan. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun DueDialog(model: AppModel, item: WorkItem, close: () -> Unit) {
    var draft by remember(item.id) { mutableStateOf(DueDraft.of(item.due)) }
    var pickingDate by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }
    var sending by remember { mutableStateOf(false) }
    val scope = rememberCoroutineScope()
    val time = rememberTimePickerState(draft.time.hour, draft.time.minute, is24Hour = true)
    val current = draft.copy(time = LocalTime.of(time.hour, time.minute))
    if (pickingDate) DateDialog(current) { picked ->
        picked?.let { draft = current.copy(date = it) }
        pickingDate = false
    }
    AlertDialog(
        onDismissRequest = { if (!sending) close() },
        title = { Text("Due") },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                TextButton({ pickingDate = true }, Modifier.testTag("due-date")) { Text(current.date.toString()) }
                TimeInput(time, Modifier.testTag("due-time"))
                OutlinedTextField(
                    current.zone, {
                        draft = current.copy(zone = it)
                        error = null
                    },
                    Modifier.fillMaxWidth().testTag("due-zone"), label = { Text("Time zone") }, singleLine = true,
                )
                (error ?: current.problem)?.let { Text(it, Modifier.testTag("due-error"), color = Theme.failure) }
            }
        },
        confirmButton = {
            TextButton({
                val due = current.toDue() ?: return@TextButton
                sending = true
                scope.launch {
                    error = model.setDue(item, due)
                    sending = false
                    if (error == null) close()
                }
            }, Modifier.testTag("due-set"), enabled = current.problem == null && !sending) { Text("Set") }
        },
        dismissButton = { TextButton(close, enabled = !sending) { Text("Cancel") } },
    )
}

/** The Material date picker; it works in UTC days. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun DateDialog(draft: DueDraft, done: (java.time.LocalDate?) -> Unit) {
    val state = rememberDatePickerState(draft.date.atStartOfDay(ZoneOffset.UTC).toInstant().toEpochMilli())
    DatePickerDialog(
        onDismissRequest = { done(null) },
        confirmButton = {
            TextButton({
                done(state.selectedDateMillis?.let { Instant.ofEpochMilli(it).atZone(ZoneOffset.UTC).toLocalDate() })
            }, Modifier.testTag("due-date-ok")) { Text("OK") }
        },
        dismissButton = { TextButton({ done(null) }) { Text("Cancel") } },
    ) { DatePicker(state) }
}
