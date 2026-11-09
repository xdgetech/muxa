package tech.xdge.muxa.app.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import tech.xdge.muxa.app.AppModel
import tech.xdge.muxa.app.InboxPrompt
import tech.xdge.muxa.app.answer
import tech.xdge.muxa.app.deleteOld
import tech.xdge.muxa.protocol.InboxAction
import tech.xdge.muxa.protocol.Proposal

/** The Inbox's questions before an action, and its failures. Shown over the Inbox screen. */
@Composable
fun InboxDialogs(model: AppModel) {
    when (val prompt = model.inboxPrompt) {
        is InboxPrompt.ApproveItem -> prompt.item.proposal?.let { p ->
            ProposalDialog(model, prompt.item.title, prompt.item.workspace, p) { model.answer(prompt.item, InboxAction.APPROVE) }
        }
        is InboxPrompt.DeleteOld -> DeleteOldDialog(model, prompt)
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

/** Names the count first; with none, there is nothing to confirm (docs/inbox.md). */
@Composable
private fun DeleteOldDialog(model: AppModel, prompt: InboxPrompt.DeleteOld) {
    val older = dayCount(prompt.days)
    if (prompt.count == 0) {
        AlertDialog(
            onDismissRequest = { model.inboxPrompt = null },
            text = { Text("No unprotected items are older than $older.", Modifier.testTag("delete-old-message")) },
            confirmButton = { TextButton({ model.inboxPrompt = null }, Modifier.testTag("delete-old-ok")) { Text("OK") } },
        )
        return
    }
    val items = if (prompt.count == 1) "1 item" else "${prompt.count} items"
    AlertDialog(
        onDismissRequest = { model.inboxPrompt = null },
        text = { Text("Delete $items older than $older? Protected items stay.", Modifier.testTag("delete-old-message")) },
        confirmButton = {
            TextButton(
                { model.deleteOld(prompt.days) }, Modifier.testTag("delete-old-confirm"),
                colors = ButtonDefaults.textButtonColors(contentColor = MaterialTheme.colorScheme.error),
            ) { Text("Delete") }
        },
        dismissButton = { TextButton({ model.inboxPrompt = null }, Modifier.testTag("delete-old-cancel")) { Text("Cancel") } },
    )
}

/** The proposal exactly as the Task will get it. Approval creates one Task. */
@Composable
private fun ProposalDialog(model: AppModel, title: String, workspace: String, p: Proposal, approve: () -> Unit) {
    AlertDialog(
        onDismissRequest = { model.inboxPrompt = null },
        title = { Text(title) },
        text = {
            Column(Modifier.fillMaxWidth().verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                Labeled("Action") { Text(p.action, Modifier.testTag("approve-action")) }
                Labeled("Target") { SelectionContainer { Text(p.target, Modifier.testTag("approve-target")) } }
                Labeled("Content") {
                    SelectionContainer { Text(p.content, Modifier.testTag("approve-content"), fontFamily = FontFamily.Monospace) }
                }
                Text(
                    "Approve creates one Task in $workspace that performs this action with this content, unchanged. " +
                        "The agent's own tools decide what it can do; muxa does not check the result.",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
        },
        confirmButton = { TextButton(approve, Modifier.testTag("approve-confirm")) { Text("Approve") } },
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
