package tech.xdge.muxa.session

import tech.xdge.muxa.protocol.AssistResult
import tech.xdge.muxa.protocol.AssistantInfo
import tech.xdge.muxa.protocol.AssistantRequest
import tech.xdge.muxa.protocol.DelegateProposal
import tech.xdge.muxa.protocol.InboxAction
import tech.xdge.muxa.protocol.InboxItem
import tech.xdge.muxa.protocol.ItemKind
import tech.xdge.muxa.protocol.ItemState
import tech.xdge.muxa.protocol.RequestStep
import tech.xdge.muxa.protocol.StaffMember
import tech.xdge.muxa.protocol.StepBy

// Staff and hand-offs as the clients show them (ADR 0051 §7). The host picks the lead, runs the steps
// and derives each step's delivery; this only picks the rows and their words. Clients say "staff
// member"; the wire says `worker`.

/** [workspace]'s staff, in configuration order; empty when it has no `staff` or the host is older. */
fun AssistantInfo?.staffOf(workspace: String?): List<StaffMember> =
    this?.delegates?.firstOrNull { it.workspace == workspace }?.staff.orEmpty()

/** The member who takes a request that names nobody: the one marked lead, else the first. */
val List<StaffMember>.lead: StaffMember? get() = firstOrNull { it.lead } ?: firstOrNull()

/** Ask Assistant offers a staff picker only when there is a choice. */
val List<StaffMember>.offersChoice: Boolean get() = size > 1

/** The picker's default, which sends no `worker`: "Let work-research decide". */
fun leadChoice(staff: List<StaffMember>): String = "Let ${staff.lead?.worker ?: "the lead"} decide"

/** "work-research → work-writer → work-review": the chain's members in step order. */
val AssistantRequest.chainLine: String get() = steps.joinToString(" → ") { it.worker }

/** Who picked the step's member. An unknown value reads as unknown, as PROTOCOL.md says. */
val StepBy.label: String
    get() = when (this) {
        StepBy.USER -> "asked by you"
        StepBy.OVERALL -> "asked by the overall assistant"
        StepBy.LEAD -> "taken by the lead"
        StepBy.HANDOFF -> "handed on"
        else -> "unknown"
    }

/** [step]'s Result in the request's current version; null while it has none. A Result without `step` is step 1. */
fun AssistantRequest.resultOf(step: Int): AssistResult? =
    results.firstOrNull { it.version == version && !it.stale && (it.step ?: 1) == step }

/** One row of Work detail's chain: the step and its Result, once stored. */
data class ChainStep(val step: RequestStep, val result: AssistResult?)

/** The current version's steps, step 1 first, each with its Result. Empty for a request without steps. */
val AssistantRequest.chain: List<ChainStep> get() = steps.map { ChainStep(it, resultOf(it.step)) }

/** The last step asks to pass the request on and no later step exists yet. */
val AssistantRequest.askedHandoff: RequestStep? get() = steps.lastOrNull()?.takeIf { it.handoff != null }

/** The `handoff` Inbox item about [step] of this request's current version, open or closed, from [items]. */
fun AssistantRequest.handoffItem(step: RequestStep, items: List<InboxItem>): InboxItem? = items.firstOrNull { item ->
    val h = item.handoff
    item.kind == ItemKind.HANDOFF && h != null && h.request == id && h.version == version && h.step == step.step
}

/** What the chain's next row says about a hand-off that no step took. */
enum class HandoffState {
    /** Its item is open: Approve starts the next step, Reject ends the chain. */
    WAITING,

    /** Rejected, or resolved by the host: the chain ended with the Results it has. */
    ENDED,
}

/** The state of the hand-off [step] asks for, from its item; null without one (`auto` starts the step itself). */
fun handoffState(item: InboxItem?): HandoffState? = when {
    item == null -> null
    item.state == ItemState.OPEN -> HandoffState.WAITING
    item.state == ItemState.ANSWERED && item.answer == InboxAction.APPROVE -> null
    else -> HandoffState.ENDED
}

/** The answers the chain row offers for a waiting hand-off, in the host's order: Approve and Reject. */
val InboxItem.handoffAnswers: List<InboxAction>
    get() = answers.filter { it == InboxAction.APPROVE || it == InboxAction.REJECT }

/**
 * The file a Result's notes replace: the lead's are the workspace's `NOTES.md`, any other member's
 * its own (ADR 0051 §5). [worker] null is the delegate of a workspace without staff.
 */
fun notesFile(worker: String?, staff: List<StaffMember>): String =
    if (worker == null || staff.isEmpty() || worker == staff.lead?.worker) NOTES_FILE else ".muxa/assistant/staff/$worker/NOTES.md"

/** Who a `delegate` proposal asks: "work-research in work", or the workspace's assistant when it names nobody. */
val DelegateProposal.addressee: String get() = worker?.let { "$it in $workspace" } ?: delegateName(workspace)

const val NOTES_FILE = ".muxa/assistant/NOTES.md"
