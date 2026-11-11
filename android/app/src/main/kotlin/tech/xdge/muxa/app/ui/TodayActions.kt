package tech.xdge.muxa.app.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.launch
import tech.xdge.muxa.app.AppModel
import tech.xdge.muxa.app.doRow
import tech.xdge.muxa.app.rowItem
import tech.xdge.muxa.app.rowSend
import tech.xdge.muxa.app.sendToday
import tech.xdge.muxa.protocol.PlanRow
import tech.xdge.muxa.session.TodaySend
import tech.xdge.muxa.session.primaryLabel

// A Today row's detail leads with what to do (ADR 0059 §2, §7): Do, the primary action, and Send
// that names who gets the text. Why, Time, Basis and Would help follow it.

/** Do, the primary action and Send; then the row's reasons. [target] tags the parts. */
@OptIn(ExperimentalLayoutApi::class)
@Composable
internal fun RowDetail(model: AppModel, row: PlanRow, target: String) {
    var sending by remember(target) { mutableStateOf<TodaySend?>(null) }
    val item = model.rowItem(row)
    val send = model.rowSend(row)
    Column(
        Modifier.padding(start = 16.dp, end = 16.dp, bottom = 10.dp).testTag("today-why-text-$target"),
        verticalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        Text("Do: ${row.next}", Modifier.testTag("today-do-$target"), style = MaterialTheme.typography.bodyMedium)
        FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Button({ model.doRow(row) }, Modifier.testTag("today-primary-$target")) {
                Text(primaryLabel(item.takeIf { row.work == null }))
            }
            send?.let { s ->
                OutlinedButton({ sending = s }, Modifier.testTag("today-send-$target")) { Text(s.label.removeSuffix("…")) }
            }
        }
        if (send == null) DetailNote(noSendNote(row), Modifier.testTag("today-no-send-$target"))
        DetailNote("Why: ${row.why}")
        DetailNote("Time: ${row.minutes.low}–${row.minutes.high} min, about ${row.minutes.estimated}")
        for (b in row.basis) BasisLine(model, b)
        for (m in row.missing) DetailNote("Would help: $m")
    }
    sending?.let { s -> TodaySendDialog(model, s, target) { sending = null } }
}

/** Why the row has no Send: a Work item needs the secretary at `work:17`; an item answers with its own buttons. */
private fun noSendNote(row: PlanRow): String =
    if (row.work != null) {
        "Send to the secretary needs muxa work level 17 and an overall assistant."
    } else {
        "Nothing to send on this item. Open it to answer."
    }

/** The text for [send], with its receiver as the title. The host's reason shows here, and the text stays. */
@Composable
internal fun TodaySendDialog(model: AppModel, send: TodaySend, target: String, close: () -> Unit) {
    var text by rememberSaveable(target) { mutableStateOf("") }
    var error by remember { mutableStateOf<String?>(null) }
    var busy by remember { mutableStateOf(false) }
    val scope = rememberCoroutineScope()
    AlertDialog(
        onDismissRequest = { if (!busy) close() },
        title = { Text(send.label.removeSuffix("…")) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                OutlinedTextField(
                    text, {
                        text = it
                        error = null
                    },
                    Modifier.fillMaxWidth().testTag("today-send-text"), maxLines = 6,
                    placeholder = { Text(placeholder(send)) },
                )
                error?.let { Text(it, Modifier.testTag("today-send-error"), color = Theme.failure) }
            }
        },
        confirmButton = {
            if (busy) {
                CircularProgressIndicator()
            } else {
                TextButton({
                    busy = true
                    scope.launch {
                        error = model.sendToday(send, text.trim())
                        busy = false
                        if (error == null) close()
                    }
                }, Modifier.testTag("today-send-confirm"), enabled = text.isNotBlank()) { Text("Send") }
            }
        },
        dismissButton = { TextButton(close, enabled = !busy) { Text("Cancel") } },
    )
}

private fun placeholder(send: TodaySend): String = when (send) {
    is TodaySend.Secretary -> "What should the secretary do about it?"
    is TodaySend.AskAgent -> "Ask the workspace's agent…"
    is TodaySend.Reply -> "Your message…"
}
