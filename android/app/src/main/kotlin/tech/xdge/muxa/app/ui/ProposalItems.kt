package tech.xdge.muxa.app.ui

import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import tech.xdge.muxa.app.AppModel
import tech.xdge.muxa.app.workItem
import tech.xdge.muxa.protocol.InboxItem
import tech.xdge.muxa.protocol.SortAction
import tech.xdge.muxa.protocol.SortProposal
import tech.xdge.muxa.session.delegateName
import tech.xdge.muxa.session.firstLine
import tech.xdge.muxa.session.label

// The overall assistant's `sort` and `delegate` items (ADR 0047 §4): what Approve would do. Approve
// runs it once on the host; Reject and Later change nothing else.

/** The proposed sort or request, with the receiving workspace, its scope and purpose. */
@Composable
internal fun OverallProposal(model: AppModel, item: InboxItem) {
    item.sort?.let { s ->
        Text("Proposed sort", Modifier.testTag("item-sort"), style = MaterialTheme.typography.titleSmall)
        val capture = model.state.captures.firstOrNull { it.id == s.capture }
        Field("Capture", capture?.firstLine ?: s.capture)
        Field("Action", sortLabel(model, s))
        s.workspace?.let { Field("Workspace", it) }
    }
    item.delegate?.let { d ->
        Text("Proposed request", Modifier.testTag("item-delegate"), style = MaterialTheme.typography.titleSmall)
        Field("To", delegateName(d.workspace))
        Field("Work item", model.workItem(d.work)?.title ?: d.work)
        Field("Scope", d.scope.label)
        Field("Purpose", d.purpose)
        Field("Why", d.reason)
    }
}

private fun sortLabel(model: AppModel, s: SortProposal): String = when (s.action) {
    SortAction.WORK -> "Make a Work item" + (s.title?.let { ": $it" } ?: "")
    SortAction.LINK -> "Add to " + (s.work?.let { model.workItem(it)?.title ?: it } ?: "a Work item")
    SortAction.NOTE -> "Keep as a note"
}
