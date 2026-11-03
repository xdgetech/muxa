package tech.xdge.muxa.app.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.DatePicker
import androidx.compose.material3.DatePickerDialog
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.SelectableDates
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TimePicker
import androidx.compose.material3.rememberDatePickerState
import androidx.compose.material3.rememberTimePickerState
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import tech.xdge.muxa.app.AppModel
import tech.xdge.muxa.app.InboxPrompt
import tech.xdge.muxa.app.approve
import tech.xdge.muxa.app.later
import tech.xdge.muxa.protocol.InboxEntry
import tech.xdge.muxa.session.LaterChoice
import tech.xdge.muxa.session.laterProblem
import java.time.Instant
import java.time.LocalDate
import java.time.LocalTime
import java.time.ZoneId
import java.time.ZoneOffset

/** The Inbox's questions before an action, and its failures. Shown over the Inbox screen. */
@Composable
fun InboxDialogs(model: AppModel) {
    when (val prompt = model.inboxPrompt) {
        is InboxPrompt.Later -> LaterDialog(model, prompt)
        is InboxPrompt.PickTime -> LaterPicker(model, prompt)
        is InboxPrompt.Approve -> ApproveDialog(model, prompt.entry)
        null -> {}
    }
    model.inboxError?.let { error ->
        AlertDialog(
            onDismissRequest = { model.inboxError = null },
            title = { Text("Can't Update the Inbox") },
            text = { Text(error, Modifier.testTag("inbox-error-message")) },
            confirmButton = { TextButton({ model.inboxError = null }) { Text("OK") } },
        )
    }
}

@Composable
private fun LaterDialog(model: AppModel, prompt: InboxPrompt.Later) {
    AlertDialog(
        onDismissRequest = { model.inboxPrompt = null },
        title = { Text("Show Again") },
        text = { Text("It leaves the Inbox until then, on every device.") },
        confirmButton = {
            Column(horizontalAlignment = Alignment.End) {
                for (choice in LaterChoice.entries) {
                    TextButton({ model.later(prompt.row, choice.until()) }, Modifier.testTag("later-${choice.name.lowercase()}")) {
                        Text(choice.title)
                    }
                }
                TextButton({ model.inboxPrompt = InboxPrompt.PickTime(prompt.row) }, Modifier.testTag("later-pick")) { Text("Pick a Time…") }
                TextButton({ model.inboxPrompt = null }) { Text("Cancel") }
            }
        },
    )
}

/** Later at a time the user picks: the day, then the time, in this device's time zone. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun LaterPicker(model: AppModel, prompt: InboxPrompt.PickTime) {
    val zone = ZoneId.systemDefault()
    val today = LocalDate.now(zone)
    // The picker's days are UTC midnights.
    val dates = rememberDatePickerState(
        initialSelectedDateMillis = today.atStartOfDay(ZoneOffset.UTC).toInstant().toEpochMilli(),
        selectableDates = object : SelectableDates {
            override fun isSelectableDate(utcTimeMillis: Long): Boolean {
                val day = Instant.ofEpochMilli(utcTimeMillis).atZone(ZoneOffset.UTC).toLocalDate()
                return !day.isBefore(today) && !day.isAfter(today.plusDays(LATER_DAYS))
            }
        },
    )
    val later = LocalTime.now(zone).plusHours(3)
    val time = rememberTimePickerState(initialHour = later.hour, initialMinute = later.minute)
    val day = dates.selectedDateMillis?.let { Instant.ofEpochMilli(it).atZone(ZoneOffset.UTC).toLocalDate() }
    val until = day?.atTime(time.hour, time.minute)?.atZone(zone)?.toEpochSecond()
    val problem = until?.let { laterProblem(it) } ?: "Pick a day."
    DatePickerDialog(
        onDismissRequest = { model.inboxPrompt = null },
        confirmButton = {
            TextButton({ until?.let { model.later(prompt.row, it) } }, Modifier.testTag("later-save"), enabled = until != null && laterProblem(until) == null) {
                Text("Save")
            }
        },
        dismissButton = { TextButton({ model.inboxPrompt = null }) { Text("Cancel") } },
    ) {
        Column(Modifier.verticalScroll(rememberScrollState()), horizontalAlignment = Alignment.CenterHorizontally) {
            DatePicker(dates, Modifier.testTag("later-date"), showModeToggle = false)
            TimePicker(time, Modifier.testTag("later-time"))
            if (until == null || laterProblem(until) != null) Text(problem, color = Theme.failure)
        }
    }
}

/** The proposal exactly as the Task will get it. Approval creates one Task. */
@Composable
private fun ApproveDialog(model: AppModel, entry: InboxEntry) {
    val p = entry.proposal ?: return
    AlertDialog(
        onDismissRequest = { model.inboxPrompt = null },
        title = { Text(entry.title) },
        text = {
            Column(Modifier.fillMaxWidth().verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                Labeled("Action") { Text(p.action, Modifier.testTag("approve-action")) }
                Labeled("Target") { SelectionContainer { Text(p.target, Modifier.testTag("approve-target")) } }
                Labeled("Content") {
                    SelectionContainer { Text(p.content, Modifier.testTag("approve-content"), fontFamily = FontFamily.Monospace) }
                }
                Text(
                    "Approve creates one Task in ${entry.workspace} that performs this action with this content, unchanged. " +
                        "The agent's own tools decide what it can do; muxa does not check the result.",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
        },
        confirmButton = { TextButton({ model.approve(entry) }, Modifier.testTag("approve-confirm")) { Text("Approve") } },
        dismissButton = { TextButton({ model.inboxPrompt = null }) { Text("Cancel") } },
    )
}

@Composable
private fun Labeled(label: String, content: @Composable () -> Unit) {
    Column {
        Text(label, style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
        content()
    }
}

/** The host refuses a Later more than 366 days ahead. */
private const val LATER_DAYS = 366L
