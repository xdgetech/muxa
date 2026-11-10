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
import tech.xdge.muxa.session.addressee
import tech.xdge.muxa.session.firstLine
import tech.xdge.muxa.session.basedOn
import tech.xdge.muxa.session.label
import tech.xdge.muxa.session.scopeLabel

// The overall assistant's `sort` and `delegate` items (ADR 0049 §4): what Approve would do. Approve
// runs it once on the host; Reject and Later change nothing else. A `hypothesis` item (ADR 0050 §3)
// shows what Confirm keeps as a policy, and its evidence by count. A `handoff` item (ADR 0051 §3)
// names both staff members; Approve starts the next step once.

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
        Field("To", d.addressee)
        Field("Work item", model.workItem(d.work)?.title ?: d.work)
        Field("Scope", d.scope.label)
        Field("Purpose", d.purpose)
        Field("Why", d.reason)
    }
    item.hypothesis?.let { h ->
        Text("Proposed policy", Modifier.testTag("item-hypothesis"), style = MaterialTheme.typography.titleSmall)
        Field("Policy", h.text)
        Field("Scope", scopeLabel(h.scope))
        Field("Confidence", h.confidence.label)
        DetailNote(basedOn(h.evidence.size), Modifier.testTag("item-hypothesis-evidence"))
        DetailNote("Confirm keeps it as your policy. Reject tells the assistant not to use or propose it again.")
    }
    item.handoff?.let { h ->
        Text("Proposed hand-off", Modifier.testTag("item-handoff"), style = MaterialTheme.typography.titleSmall)
        Field("From", "${h.from} (step ${h.step})")
        Field("To", "${h.worker} (step ${h.step + 1})")
        Field("Work item", model.workItem(h.work)?.title ?: h.work)
        Field("Purpose", h.purpose)
        DetailNote("Approve starts ${h.worker}'s step. Reject ends the chain with the Results it has.")
    }
}

private fun sortLabel(model: AppModel, s: SortProposal): String = when (s.action) {
    SortAction.WORK -> "Make a Work item" + (s.title?.let { ": $it" } ?: "")
    SortAction.LINK -> "Add to " + (s.work?.let { model.workItem(it)?.title ?: it } ?: "a Work item")
    SortAction.NOTE -> "Keep as a note"
}
