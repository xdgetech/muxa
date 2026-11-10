package tech.xdge.muxa.app

import tech.xdge.muxa.protocol.AssistantRequest
import tech.xdge.muxa.protocol.InboxAction
import tech.xdge.muxa.protocol.InboxItem
import tech.xdge.muxa.protocol.RequestStep
import tech.xdge.muxa.protocol.StaffMember
import tech.xdge.muxa.session.handoffItem
import tech.xdge.muxa.session.offersChoice
import tech.xdge.muxa.session.staffOf

// Staff and hand-offs (ADR 0051). The host runs the chain and answers the hand-off; a lower host
// keeps today's UI: no picker, no chain rows, no `handoff` items.

/** The host asks named staff members and runs a request as a chain of steps (`work:12`). */
val AppModel.canAskStaff: Boolean get() = isConnected && state.canAskStaff

/** [workspace]'s staff, in configuration order; empty below `work:12` or without `staff`. */
fun AppModel.staffOf(workspace: String?): List<StaffMember> = if (canAskStaff) state.assistant.staffOf(workspace) else emptyList()

/** The members Ask Assistant offers: only when there is more than one. */
fun AppModel.staffChoice(workspace: String?): List<StaffMember> = staffOf(workspace).takeIf { it.offersChoice }.orEmpty()

/** The `handoff` item about [step] of [request], as the Inbox lists it. */
fun AppModel.handoffItem(request: AssistantRequest, step: RequestStep): InboxItem? =
    if (canAskStaff) request.handoffItem(step, state.inboxItems) else null

/** Approve or Reject on a waiting hand-off in Work detail: the same answer as in the Inbox. */
fun AppModel.answerHandoff(item: InboxItem, action: InboxAction) = sendAnswer(item, action, text = null, option = null)
