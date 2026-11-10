package tech.xdge.muxa.app.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.DatePicker
import androidx.compose.material3.DatePickerDialog
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilterChip
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.rememberDatePickerState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.launch
import tech.xdge.muxa.app.AppModel
import tech.xdge.muxa.app.addPolicy
import tech.xdge.muxa.app.editPolicy
import tech.xdge.muxa.app.policyScopes
import tech.xdge.muxa.protocol.Policy
import tech.xdge.muxa.session.ALL_SCOPE
import tech.xdge.muxa.session.PolicyDraft
import tech.xdge.muxa.session.PolicyEnd
import tech.xdge.muxa.session.scopeLabel
import java.time.Instant
import java.time.LocalDate
import java.time.ZoneOffset

// Add or Edit a policy (ADR 0050 §1, §7): the user's sentence, the workspace it applies to, and
// when it starts and ends. "This week" ends on Sunday night.

/** [policy] null adds one. Save sends it; the host checks it again and says why it refused. */
@Composable
internal fun PolicyDialog(model: AppModel, policy: Policy?, close: () -> Unit) {
    var draft by remember(policy?.id) { mutableStateOf(policy?.let { PolicyDraft.of(it) } ?: PolicyDraft()) }
    var error by remember { mutableStateOf<String?>(null) }
    var sending by remember { mutableStateOf(false) }
    val scope = rememberCoroutineScope()
    val problem = draft.problem(System.currentTimeMillis() / 1000)
    val change = { d: PolicyDraft ->
        draft = d
        error = null
    }
    AlertDialog(
        onDismissRequest = { if (!sending) close() },
        title = { Text(if (policy == null) "Add Policy" else "Edit Policy") },
        text = {
            Column(Modifier.verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                OutlinedTextField(
                    draft.text, { change(draft.copy(text = it)) }, Modifier.fillMaxWidth().testTag("policy-text"),
                    label = { Text("In your words") }, placeholder = { Text("This week, recruiting comes first.") }, maxLines = 4,
                )
                ScopePicker(model, draft.scope) { change(draft.copy(scope = it)) }
                StartPicker(draft.from) { change(draft.copy(from = it)) }
                EndPicker(draft.end) { change(draft.copy(end = it)) }
                (error ?: problem.takeIf { draft.text.isNotEmpty() })?.let { Text(it, Modifier.testTag("policy-error"), color = Theme.failure) }
            }
        },
        confirmButton = {
            TextButton({
                sending = true
                scope.launch {
                    error = if (policy == null) model.addPolicy(draft) else model.editPolicy(policy, draft)
                    sending = false
                    if (error == null) close()
                }
            }, Modifier.testTag("policy-save"), enabled = problem == null && !sending) { Text("Save") }
        },
        dismissButton = { TextButton(close, enabled = !sending) { Text("Cancel") } },
    )
}

/** All workspaces, or one; never the overall assistant's. */
@Composable
private fun ScopePicker(model: AppModel, scope: String, pick: (String) -> Unit) {
    var open by remember { mutableStateOf(false) }
    Box {
        OutlinedButton({ open = true }, Modifier.fillMaxWidth().testTag("policy-scope")) { Text("Applies to: ${scopeLabel(scope)}") }
        DropdownMenu(open, { open = false }) {
            for (name in listOf(ALL_SCOPE) + model.policyScopes) {
                DropdownMenuItem({ Text(scopeLabel(name)) }, {
                    open = false
                    pick(name)
                }, Modifier.testTag("policy-scope-$name"))
            }
        }
    }
}

/** Now, or from a day. */
@Composable
private fun StartPicker(from: LocalDate?, pick: (LocalDate?) -> Unit) {
    var picking by remember { mutableStateOf(false) }
    Row(verticalAlignment = Alignment.CenterVertically) {
        Text("Starts", Modifier.weight(1f))
        TextButton({ picking = true }, Modifier.testTag("policy-from")) { Text(from?.toString() ?: "Now") }
        if (from != null) TextButton({ pick(null) }, Modifier.testTag("policy-from-now")) { Text("Now") }
    }
    if (picking) DayDialog(from ?: LocalDate.now(), "policy-from-ok") { day ->
        day?.let(pick)
        picking = false
    }
}

/** No end, This week, or Until a day. */
@Composable
private fun EndPicker(end: PolicyEnd, pick: (PolicyEnd) -> Unit) {
    var picking by remember { mutableStateOf(false) }
    Text("Ends")
    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        FilterChip(end == PolicyEnd.None, { pick(PolicyEnd.None) }, { Text("No end") }, Modifier.testTag("policy-end-none"))
        FilterChip(end == PolicyEnd.ThisWeek, { pick(PolicyEnd.ThisWeek) }, { Text("This week") }, Modifier.testTag("policy-end-week"))
        FilterChip(end is PolicyEnd.Until, { picking = true }, { Text((end as? PolicyEnd.Until)?.date?.toString() ?: "Until…") }, Modifier.testTag("policy-end-until"))
    }
    if (picking) DayDialog((end as? PolicyEnd.Until)?.date ?: LocalDate.now().plusDays(1), "policy-until-ok") { day ->
        day?.let { pick(PolicyEnd.Until(it)) }
        picking = false
    }
}

/** The Material date picker; it works in UTC days. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun DayDialog(day: LocalDate, okTag: String, done: (LocalDate?) -> Unit) {
    val state = rememberDatePickerState(day.atStartOfDay(ZoneOffset.UTC).toInstant().toEpochMilli())
    DatePickerDialog(
        onDismissRequest = { done(null) },
        confirmButton = {
            TextButton({ done(state.selectedDateMillis?.let { Instant.ofEpochMilli(it).atZone(ZoneOffset.UTC).toLocalDate() }) }, Modifier.testTag(okTag)) { Text("OK") }
        },
        dismissButton = { TextButton({ done(null) }) { Text("Cancel") } },
    ) { DatePicker(state) }
}
