package tech.xdge.muxa.app.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ExposedDropdownMenuAnchorType
import androidx.compose.material3.ExposedDropdownMenuBox
import androidx.compose.material3.ExposedDropdownMenuDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import tech.xdge.muxa.app.AppModel
import tech.xdge.muxa.app.answerHandoff
import tech.xdge.muxa.app.handoffItem
import tech.xdge.muxa.app.staffChoice
import tech.xdge.muxa.app.staffOf
import tech.xdge.muxa.protocol.AssistantRequest
import tech.xdge.muxa.protocol.InboxAction
import tech.xdge.muxa.protocol.RequestStep
import tech.xdge.muxa.protocol.StaffMember
import tech.xdge.muxa.session.ChainStep
import tech.xdge.muxa.session.HandoffState
import tech.xdge.muxa.session.askedHandoff
import tech.xdge.muxa.session.chain
import tech.xdge.muxa.session.chainLine
import tech.xdge.muxa.session.handoffAnswers
import tech.xdge.muxa.session.handoffState
import tech.xdge.muxa.session.label
import tech.xdge.muxa.session.leadChoice
import tech.xdge.muxa.session.notesFile

// A workspace's staff in the app (ADR 0051 §7): the picker on Ask Assistant, and a request's chain in
// Work detail. Each step is one run with its own delivery, Result and notes.

/** The chain line, each step as a row, then the hand-off the last step asks for, greyed. */
@Composable
internal fun ChainSection(model: AppModel, request: AssistantRequest) {
    val staff = model.staffOf(request.workspace)
    DetailNote(request.chainLine, Modifier.testTag("request-chain-${request.id}"))
    for (row in request.chain) StepRow(model, request, row, staff)
    request.askedHandoff?.let { HandoffRow(model, request, it) }
}

/** Step k, its member, its delivery with its age and who picked the member; its Result and notes once stored. */
@Composable
private fun StepRow(model: AppModel, request: AssistantRequest, row: ChainStep, staff: List<StaffMember>) {
    val step = row.step
    val key = "${request.id}-${step.step}"
    Column(Modifier.fillMaxWidth().testTag("chain-step-$key"), verticalArrangement = Arrangement.spacedBy(4.dp)) {
        DetailHeader("Step ${step.step} · ${step.worker}")
        Text(
            listOf(step.delivery.label, AgentSummary.age(step.deliveryAt), step.by.label).joinToString(" · "),
            Modifier.testTag("chain-delivery-$key"),
            style = MaterialTheme.typography.bodySmall,
            color = deliveryColor(step.delivery),
        )
        val result = row.result ?: return@Column
        ResultView(model, request, result, tag = "chain-result-$key")
        val file = notesFile(step.worker, staff)
        NotesProposal(model, request, result, "${step.worker} proposes these notes for later runs. They replace $file.", "chain-notes-$key")
    }
}

/**
 * The next step a hand-off asks for: greyed, with Approve and Reject while its item waits; one line
 * once the chain ended without it. Nothing under `auto`: the host starts the step itself.
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun HandoffRow(model: AppModel, request: AssistantRequest, from: RequestStep) {
    val handoff = from.handoff ?: return
    val item = model.handoffItem(request, from)
    val state = handoffState(item) ?: return
    Column(Modifier.fillMaxWidth().testTag("chain-handoff-${request.id}"), verticalArrangement = Arrangement.spacedBy(4.dp)) {
        Column(Modifier.alpha(GREYED)) {
            DetailHeader("Step ${from.step + 1} · ${handoff.worker}")
            val note = when (state) {
                HandoffState.WAITING -> "Waits for your approval. ${from.worker} asks: ${handoff.purpose}"
                HandoffState.ENDED -> "Not started. The chain ended with the Results above."
            }
            DetailNote(note, Modifier.testTag("chain-handoff-state-${request.id}"))
        }
        if (state == HandoffState.WAITING && item != null) {
            FlowRow(horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                for (action in item.handoffAnswers) {
                    TextButton(
                        { model.answerHandoff(item, action) },
                        Modifier.testTag("chain-${if (action == InboxAction.APPROVE) "approve" else "reject"}-${request.id}"),
                        enabled = model.answeringItem == null,
                    ) { Text(if (action == InboxAction.APPROVE) "Approve" else "Reject") }
                }
            }
        }
    }
}

/**
 * Who answers, when the workspace has more than one staff member: "Let <lead> decide" sends no
 * `worker`. Each member shows its description under its name. [tag] names the field and, with a
 * suffix, each choice.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
internal fun StaffPicker(model: AppModel, workspace: String?, worker: String?, tag: String, pick: (String?) -> Unit) {
    val staff = model.staffChoice(workspace)
    if (staff.isEmpty()) return
    var picking by remember { mutableStateOf(false) }
    val chosen = staff.firstOrNull { it.worker == worker }
    ExposedDropdownMenuBox(picking, { picking = it }) {
        OutlinedTextField(
            chosen?.worker ?: leadChoice(staff), {},
            Modifier.fillMaxWidth().menuAnchor(ExposedDropdownMenuAnchorType.PrimaryNotEditable).testTag(tag),
            readOnly = true, singleLine = true, label = { Text("Staff member") },
            supportingText = chosen?.let { { Text(it.description, maxLines = 2, overflow = TextOverflow.Ellipsis) } },
            trailingIcon = { ExposedDropdownMenuDefaults.TrailingIcon(picking) },
        )
        ExposedDropdownMenu(picking, { picking = false }) {
            DropdownMenuItem({ Text(leadChoice(staff)) }, {
                pick(null)
                picking = false
            }, modifier = Modifier.testTag("$tag-lead"))
            for (member in staff) {
                DropdownMenuItem({
                    Column {
                        Text(member.worker)
                        Text(
                            member.description, style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant, maxLines = 2, overflow = TextOverflow.Ellipsis,
                        )
                    }
                }, {
                    pick(member.worker)
                    picking = false
                }, modifier = Modifier.testTag("$tag-${member.worker}"))
            }
        }
    }
}

/** A step that has not started reads as greyed. */
private const val GREYED = 0.6f
