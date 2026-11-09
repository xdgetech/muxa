package tech.xdge.muxa.app.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
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
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.launch
import tech.xdge.muxa.app.AppModel
import tech.xdge.muxa.app.hasToday
import tech.xdge.muxa.app.item
import tech.xdge.muxa.app.openPlanRow
import tech.xdge.muxa.app.planIsNew
import tech.xdge.muxa.app.planNow
import tech.xdge.muxa.app.shownPlan
import tech.xdge.muxa.app.workItem
import tech.xdge.muxa.protocol.Plan
import tech.xdge.muxa.protocol.PlanRow
import tech.xdge.muxa.session.dueSoonLine
import tech.xdge.muxa.session.label
import tech.xdge.muxa.session.line
import tech.xdge.muxa.session.marks
import tech.xdge.muxa.session.planStatus
import tech.xdge.muxa.session.planning
import tech.xdge.muxa.session.target

// Today, above Decisions (ADR 0047 §5): the newest plan's rows. Its keys are row positions, so a
// newer plan keeps the list's scroll; nothing here changes the selection.

/** The header with New and Plan now, the newest plan's state, the rows, then the Due soon line. */
fun LazyListScope.todaySection(model: AppModel) {
    if (!model.hasToday) return
    val plan = model.shownPlan
    val status = planStatus(model.state.plans)
    item(key = "h-today") { TodayHeader(model) }
    status?.let { item(key = "today-status") { TodayNote(it, "today-status") } }
    when {
        plan == null && status == null -> item(key = "today-empty") { TodayNote("No plan yet. Plan now asks the overall assistant.", "today-empty") }
        plan != null && plan.top.isEmpty() -> item(key = "today-empty") { TodayNote("Nothing to rank in the newest plan.", "today-empty") }
    }
    val rows = plan?.top.orEmpty()
    rows.forEachIndexed { i, row ->
        item(key = "today-$i") {
            TodayRow(model, plan ?: return@item, row)
            HorizontalDivider()
        }
    }
    plan?.dueSoonLine { id -> model.workItem(id)?.title ?: id }?.let { line ->
        item(key = "today-due-soon") { TodayNote(line, "today-due-soon") }
    }
}

@Composable
private fun TodayHeader(model: AppModel) {
    val planning = model.state.plans.planning()
    Row(Modifier.testTag("inbox-section-today")) {
        SectionHeader("Today") {
            if (model.planIsNew) {
                Text(
                    "New",
                    Modifier.background(MaterialTheme.colorScheme.primary, RoundedCornerShape(8.dp)).padding(horizontal = 8.dp, vertical = 2.dp)
                        .testTag("today-new"),
                    color = MaterialTheme.colorScheme.onPrimary,
                    style = MaterialTheme.typography.labelSmall,
                )
            }
            TextButton({ model.planNowShown = true }, Modifier.testTag("today-plan-now"), enabled = !planning) { Text("Plan now") }
        }
    }
}

@Composable
private fun TodayNote(text: String, tag: String) = Text(
    text,
    Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 8.dp).testTag(tag),
    style = MaterialTheme.typography.bodySmall,
    color = MaterialTheme.colorScheme.onSurfaceVariant,
)

/** The target's title, why, the minutes and the host's marks; Why opens the basis and what is missing. */
@Composable
private fun TodayRow(model: AppModel, plan: Plan, row: PlanRow) {
    val target = row.target ?: return
    var why by remember(plan.id, target) { mutableStateOf(false) }
    val work = row.work?.let(model::workItem)
    val title = work?.title ?: row.item?.let { model.item(it)?.question } ?: target
    val marks = work?.let(plan::marks).orEmpty()
    Column(Modifier.fillMaxWidth().testTag("today-row-$target")) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Column(
                Modifier.weight(1f).clickable { model.openPlanRow(row) }.padding(start = 16.dp, top = 10.dp, bottom = 10.dp).testTag("today-open-$target"),
            ) {
                Text(title, maxLines = 2, overflow = TextOverflow.Ellipsis)
                Text(row.why, style = MaterialTheme.typography.bodySmall, maxLines = 2, overflow = TextOverflow.Ellipsis)
                Text(
                    (listOf(row.minutes.label) + marks).joinToString(" · "),
                    Modifier.testTag("today-minutes-$target"),
                    style = MaterialTheme.typography.labelMedium,
                    color = if (marks.isEmpty()) MaterialTheme.colorScheme.onSurfaceVariant else Theme.attention,
                )
            }
            TextButton({ why = !why }, Modifier.testTag("today-why-$target")) { Text(if (why) "Hide" else "Why") }
        }
        if (why) {
            Column(Modifier.padding(start = 16.dp, end = 16.dp, bottom = 10.dp).testTag("today-why-text-$target"), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                DetailNote("Next: ${row.next}")
                for (b in row.basis) DetailNote("• ${b.line}")
                for (m in row.missing) DetailNote("Would help: $m")
            }
        }
    }
}

/** Plan now with the time the user has, optional. The plan comes when the overall assistant is done. */
@Composable
fun PlanNowDialog(model: AppModel) {
    if (!model.planNowShown) return
    var minutes by rememberSaveable { mutableStateOf("") }
    var error by remember { mutableStateOf<String?>(null) }
    var sending by remember { mutableStateOf(false) }
    val scope = rememberCoroutineScope()
    val parsed = minutes.trim().toIntOrNull()
    val problem = when {
        minutes.isBlank() -> null
        parsed == null || parsed !in 1..MAX_MINUTES -> "Minutes are a number from 1 to $MAX_MINUTES."
        else -> null
    }
    AlertDialog(
        onDismissRequest = { if (!sending) model.planNowShown = false },
        title = { Text("Plan Now") },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text("The overall assistant ranks what to do next. The plan shows under Today when it is ready.")
                OutlinedTextField(
                    minutes, {
                        minutes = it
                        error = null
                    },
                    Modifier.fillMaxWidth().testTag("plan-now-minutes"),
                    label = { Text("Minutes you have (optional)") }, singleLine = true,
                    keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number),
                )
                (error ?: problem)?.let { Text(it, Modifier.testTag("plan-now-error"), color = Theme.failure) }
            }
        },
        confirmButton = {
            if (sending) {
                CircularProgressIndicator()
            } else {
                TextButton({
                    sending = true
                    scope.launch {
                        error = model.planNow(parsed)
                        sending = false
                        if (error == null) model.planNowShown = false
                    }
                }, Modifier.testTag("plan-now-confirm"), enabled = problem == null) { Text("Plan") }
            }
        },
        dismissButton = { TextButton({ model.planNowShown = false }, enabled = !sending) { Text("Cancel") } },
    )
}

private const val MAX_MINUTES = 1440
