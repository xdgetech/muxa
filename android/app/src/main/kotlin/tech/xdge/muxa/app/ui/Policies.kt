package tech.xdge.muxa.app.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.testTagsAsResourceId
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import kotlinx.coroutines.launch
import tech.xdge.muxa.app.AppModel
import tech.xdge.muxa.app.deletePolicy
import tech.xdge.muxa.app.forget
import tech.xdge.muxa.app.memory
import tech.xdge.muxa.app.rowTitle
import tech.xdge.muxa.app.tap
import tech.xdge.muxa.protocol.Correction
import tech.xdge.muxa.protocol.Hypothesis
import tech.xdge.muxa.protocol.HypothesisState
import tech.xdge.muxa.protocol.InboxAction
import tech.xdge.muxa.protocol.Policy
import tech.xdge.muxa.session.askingAbout
import tech.xdge.muxa.session.basedOn
import tech.xdge.muxa.session.canForget
import tech.xdge.muxa.session.label
import tech.xdge.muxa.session.scopeLabel
import tech.xdge.muxa.session.span
import tech.xdge.muxa.session.summary

/**
 * Policies (ADR 0050 §7): the one place that shows what the overall assistant remembers. The user's
 * policies with Add, Edit and Delete; the assistant's hypotheses with Confirm, Reject and Forget; the
 * recent corrections with Forget. It covers the window like Settings.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun PoliciesScreen(model: AppModel) {
    if (!model.policiesShown) return
    var editing by remember { mutableStateOf<PolicyEdit?>(null) }
    val close = { model.policiesShown = false }
    Dialog(onDismissRequest = close, properties = DialogProperties(usePlatformDefaultWidth = false, decorFitsSystemWindows = false)) {
        DialogStatusBar()
        // A dialog is its own window: it needs the tag setting the main window has (Root).
        Surface(Modifier.fillMaxSize().semantics { testTagsAsResourceId = true }, color = MaterialTheme.colorScheme.background) {
            Column(Modifier.fillMaxSize().navigationBarsPadding().testTag("policies")) {
                TopAppBar(
                    title = { Text("Policies") },
                    navigationIcon = { IconButton(close, Modifier.testTag("policies-done")) { Icon(Icons.close, "Done") } },
                    actions = { IconButton({ editing = PolicyEdit(null) }, Modifier.testTag("policies-add")) { Icon(Icons.add, "Add Policy") } },
                )
                LazyColumn(Modifier.fillMaxWidth().testTag("policies-list")) {
                    policies(model) { editing = PolicyEdit(it) }
                    hypotheses(model)
                    corrections(model)
                    item(key = "forget-note") {
                        DetailNote(FORGET_NOTE, Modifier.padding(16.dp))
                    }
                }
            }
        }
        editing?.let { edit -> PolicyDialog(model, edit.policy) { editing = null } }
        ForgetDialog(model)
    }
}

/** The policy the sheet edits; null adds one. */
private class PolicyEdit(val policy: Policy?)

private fun LazyListScope.policies(model: AppModel, edit: (Policy) -> Unit) {
    val policies = model.memory.policies
    item(key = "h-policies") { SectionHeader("Your policies") }
    if (policies.isEmpty()) {
        item(key = "no-policies") { Empty("None yet. A policy tells the assistant what matters, in your words: \"This week, recruiting comes first.\"") }
    }
    items(policies, key = { it.id }) { p -> PolicyRow(model, p) { edit(p) } }
}

@Composable
private fun PolicyRow(model: AppModel, policy: Policy, edit: () -> Unit) {
    val scope = rememberCoroutineScope()
    var sending by remember(policy.id) { mutableStateOf(false) }
    Record(policy.text, policy.span(System.currentTimeMillis() / 1000), "policy-${policy.id}") {
        TextButton(edit, Modifier.testTag("policy-edit-${policy.id}"), enabled = !sending) { Text("Edit") }
        TextButton({
            sending = true
            scope.launch {
                model.deletePolicy(policy)?.let { model.inboxError = it }
                sending = false
            }
        }, Modifier.testTag("policy-delete-${policy.id}"), enabled = !sending) { Text("Delete", color = Theme.failure) }
    }
}

private fun LazyListScope.hypotheses(model: AppModel) {
    val hypotheses = model.memory.hypotheses
    item(key = "h-hypotheses") { SectionHeader("What the assistant noticed") }
    if (hypotheses.isEmpty()) item(key = "no-hypotheses") { Empty("No hypotheses. The assistant proposes some from your corrections.") }
    items(hypotheses, key = { it.id }) { h -> HypothesisRow(model, h) }
}

/** Confirm and Reject answer its Inbox item, so they show while the item is open. */
@Composable
private fun HypothesisRow(model: AppModel, h: Hypothesis) {
    val item = model.state.inboxItems.askingAbout(h)?.takeIf { h.state == HypothesisState.PROPOSED }
    val busy = model.answeringItem != null
    val detail = listOf(scopeLabel(h.scope), h.confidence.label, h.basedOn.lowercase(), h.state.label).joinToString(" · ")
    Record(h.text, detail, "hypothesis-${h.id}") {
        if (item != null) {
            TextButton({ model.tap(item, InboxAction.CONFIRM) }, Modifier.testTag("hypothesis-confirm-${h.id}"), enabled = !busy) { Text("Confirm") }
            TextButton({ model.tap(item, InboxAction.REJECT) }, Modifier.testTag("hypothesis-reject-${h.id}"), enabled = !busy) { Text("Reject") }
        }
        if (h.canForget) {
            TextButton({ model.forgetting = h.id }, Modifier.testTag("hypothesis-forget-${h.id}")) { Text("Forget") }
        }
    }
}

private fun LazyListScope.corrections(model: AppModel) {
    val corrections = model.memory.corrections
    item(key = "h-corrections") { SectionHeader("Recent corrections") }
    if (corrections.isEmpty()) item(key = "no-corrections") { Empty("None. Moving, pinning or rejecting a planned row records one.") }
    items(corrections, key = { it.id }) { c -> CorrectionRow(model, c) }
}

@Composable
private fun CorrectionRow(model: AppModel, c: Correction) {
    Record(c.summary(model::rowTitle), AgentSummary.age(c.at), "correction-${c.id}") {
        TextButton({ model.forgetting = c.id }, Modifier.testTag("correction-forget-${c.id}")) { Text("Forget") }
    }
}

/** One record: its text, a detail line, and its actions on the right. */
@Composable
private fun Record(text: String, detail: String, tag: String, actions: @Composable () -> Unit) {
    Row(Modifier.fillMaxWidth().padding(start = 16.dp, end = 4.dp, top = 8.dp, bottom = 8.dp).testTag(tag), verticalAlignment = Alignment.CenterVertically) {
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(text)
            DetailNote(detail)
        }
        Column(horizontalAlignment = Alignment.End) { actions() }
    }
    HorizontalDivider()
}

@Composable
private fun Empty(text: String) = DetailNote(text, Modifier.padding(horizontal = 16.dp, vertical = 8.dp))

/** Forget asks first, and says what it does not undo (ADR 0050 §5). */
@Composable
fun ForgetDialog(model: AppModel) {
    val record = model.forgetting ?: return
    var error by remember(record) { mutableStateOf<String?>(null) }
    var sending by remember(record) { mutableStateOf(false) }
    val scope = rememberCoroutineScope()
    AlertDialog(
        onDismissRequest = { if (!sending) model.forgetting = null },
        title = { Text("Forget This?") },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(FORGET_NOTE, Modifier.testTag("forget-message"))
                error?.let { Text(it, Modifier.testTag("forget-error"), color = Theme.failure) }
            }
        },
        confirmButton = {
            TextButton({
                sending = true
                scope.launch {
                    error = model.forget(record)
                    sending = false
                    if (error == null) model.forgetting = null
                }
            }, Modifier.testTag("forget-confirm"), enabled = !sending) { Text("Forget", color = Theme.failure) }
        },
        dismissButton = { TextButton({ model.forgetting = null }, enabled = !sending) { Text("Cancel") } },
    )
}

private const val FORGET_NOTE =
    "Forget removes it from what the assistant reads next, with the guesses based on it. Plans made before keep their reasons."
