package tech.xdge.muxa.app.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import tech.xdge.muxa.app.AppModel
import tech.xdge.muxa.app.itemSession
import tech.xdge.muxa.app.openItemSession
import tech.xdge.muxa.protocol.InboxItem
import tech.xdge.muxa.session.PaneSession
import tech.xdge.muxa.session.isPlan
import tech.xdge.muxa.session.proposedTask

// The assistant proposes Tasks (ADR 0060 §8): a Proposed Task card on a `result` or `new-task` item,
// and a plan item's words. Start Task, Send and Done are the item's own answers.

/** Title, workspace, repo and goal of the Task the item offers; it starts in plan mode. */
@Composable
internal fun ProposedTaskCard(item: InboxItem) {
    val task = item.proposedTask ?: return
    Column(Modifier.fillMaxWidth().testTag("item-proposed-task"), verticalArrangement = Arrangement.spacedBy(4.dp)) {
        Text("Proposed Task", style = MaterialTheme.typography.titleSmall)
        Text(task.title, Modifier.testTag("item-proposed-task-title"), style = MaterialTheme.typography.bodyLarge)
        val where = listOf(task.workspace, task.repo?.let { "repo $it" } ?: "its own folder")
        DetailNote(where.joinToString(" · "), Modifier.testTag("item-proposed-task-where"))
        SelectionContainer { Text(task.goal, Modifier.testTag("item-proposed-task-goal")) }
        task.worker?.let { DetailNote("Proposed by $it") }
        DetailNote(
            "Starts in plan mode: its agent plans first, and you approve the plan in the Inbox.",
            Modifier.testTag("item-proposed-task-plan"),
        )
    }
}

/** What Approve plan does on a plan item, which no other answer says. */
@Composable
internal fun PlanNote(item: InboxItem) {
    if (!item.isPlan) return
    DetailNote(
        "Approve plan leaves plan mode, and the agent asks before each edit. Keep planning sends nothing.",
        Modifier.testTag("item-plan-note"),
    )
}

/** Approve plan pressed nothing (ADR 0060 §5): the user approves in the Task's terminal. */
@Composable
internal fun ApproveInTerminalDialog(model: AppModel, item: InboxItem) {
    val session = model.itemSession(item)
    AlertDialog(
        onDismissRequest = { model.inboxPrompt = null },
        title = { Text("Approve in the Terminal") },
        text = { Text(PaneSession.APPROVE_IN_TERMINAL, Modifier.testTag("approve-in-terminal-message")) },
        confirmButton = {
            if (session != null) {
                TextButton({
                    model.inboxPrompt = null
                    model.openItemSession(item, AppModel.PaneView.AGENT)
                }, Modifier.testTag("approve-in-terminal-open")) { Text("Open Session") }
            }
        },
        dismissButton = { TextButton({ model.inboxPrompt = null }, Modifier.testTag("approve-in-terminal-ok")) { Text("OK") } },
    )
}
