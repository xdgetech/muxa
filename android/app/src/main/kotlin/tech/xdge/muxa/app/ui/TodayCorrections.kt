package tech.xdge.muxa.app.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
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
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.launch
import tech.xdge.muxa.app.AppModel
import tech.xdge.muxa.app.dismissTodayRow
import tech.xdge.muxa.app.memory
import tech.xdge.muxa.app.moveTodayRow
import tech.xdge.muxa.app.rowTitle
import tech.xdge.muxa.protocol.Basis
import tech.xdge.muxa.protocol.PlanRow
import tech.xdge.muxa.session.Cited
import tech.xdge.muxa.session.label
import tech.xdge.muxa.session.line
import tech.xdge.muxa.session.summary
import tech.xdge.muxa.session.target

// Today's corrections and its memory (ADR 0050 §5, §7): move a row, take it out for today with an
// optional reason, and see the policies, hypotheses and corrections a row's reasons cite.

/** Move Up, Move Down and Not Today. Each change is one correction on the host. */
@Composable
internal fun TodayRowMenu(model: AppModel, target: String, index: Int, count: Int) {
    var open by remember { mutableStateOf(false) }
    var dismissing by remember { mutableStateOf(false) }
    val scope = rememberCoroutineScope()
    val move = { offset: Int ->
        open = false
        scope.launch { model.moveTodayRow(target, offset)?.let { model.inboxError = it } }
        Unit
    }
    Box {
        IconButton({ open = true }, Modifier.testTag("today-more-$target")) { Icon(Icons.more, "Row Actions") }
        DropdownMenu(open, { open = false }) {
            if (index > 0) {
                DropdownMenuItem({ Text("Move Up") }, { move(-1) }, Modifier.testTag("today-up-$target"), leadingIcon = { Icon(Icons.moveUp, null) })
            }
            if (index < count - 1) {
                DropdownMenuItem({ Text("Move Down") }, { move(1) }, Modifier.testTag("today-down-$target"), leadingIcon = { Icon(Icons.moveDown, null) })
            }
            DropdownMenuItem({ Text("Not Today") }, {
                open = false
                dismissing = true
            }, Modifier.testTag("today-not-today-$target"), leadingIcon = { Icon(Icons.later, null) })
        }
    }
    if (dismissing) NotTodayDialog(model, target) { dismissing = false }
}

/** Not Today with an optional reason; the row collapses to one line. Nothing asks for a reason. */
@Composable
private fun NotTodayDialog(model: AppModel, target: String, close: () -> Unit) {
    var reason by rememberSaveable(target) { mutableStateOf("") }
    var error by remember { mutableStateOf<String?>(null) }
    var sending by remember { mutableStateOf(false) }
    val scope = rememberCoroutineScope()
    AlertDialog(
        onDismissRequest = { if (!sending) close() },
        title = { Text("Not Today") },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(model.rowTitle(target), maxLines = 2, overflow = TextOverflow.Ellipsis)
                ReasonField(reason, "not-today-reason") { reason = it }
                error?.let { Text(it, Modifier.testTag("not-today-error"), color = Theme.failure) }
            }
        },
        confirmButton = {
            if (sending) {
                CircularProgressIndicator()
            } else {
                TextButton({
                    sending = true
                    scope.launch {
                        error = model.dismissTodayRow(target, reason)
                        sending = false
                        if (error == null) close()
                    }
                }, Modifier.testTag("not-today-confirm")) { Text("Not Today") }
            }
        },
        dismissButton = { TextButton(close, enabled = !sending) { Text("Cancel") } },
    )
}

/** A row the user removed for today: one line. */
@Composable
internal fun DismissedRow(model: AppModel, row: PlanRow) {
    val target = row.target ?: return
    DetailNote(
        "Not today · ${model.rowTitle(target)}",
        Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 8.dp).testTag("today-dismissed-$target"),
    )
}

/**
 * One reason of a row. A policy, hypothesis or correction shows the record's own words, with Edit
 * or Forget; an id with no record shows "forgotten".
 */
@Composable
internal fun BasisLine(model: AppModel, basis: Basis) {
    val cited = model.memory.cited(basis)
    val id = basis.source.orEmpty()
    val text = when (cited) {
        null -> return DetailNote("• ${basis.line}")
        is Cited.PolicyRecord -> "Your policy: ${cited.policy.text}"
        is Cited.HypothesisRecord -> "Hypothesis (${cited.hypothesis.state.label}): ${cited.hypothesis.text}"
        is Cited.CorrectionRecord -> cited.correction.summary(model::rowTitle)
        is Cited.Forgotten -> "${basis.line.substringBefore(':')}: forgotten"
    }
    Row(Modifier.fillMaxWidth().testTag("today-cite-$id"), verticalAlignment = Alignment.CenterVertically) {
        DetailNote("• $text", Modifier.weight(1f))
        when (cited) {
            is Cited.PolicyRecord -> TextButton({ model.policiesShown = true }, Modifier.testTag("today-edit-$id")) { Text("Edit") }
            is Cited.HypothesisRecord, is Cited.CorrectionRecord ->
                TextButton({ model.forgetting = id }, Modifier.testTag("today-forget-$id")) { Text("Forget") }
            else -> {}
        }
    }
}
