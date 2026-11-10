package tech.xdge.muxa.app

import tech.xdge.muxa.protocol.AssistResult
import tech.xdge.muxa.protocol.AssistantRequest
import tech.xdge.muxa.protocol.Due
import tech.xdge.muxa.protocol.Plan
import tech.xdge.muxa.protocol.PlanRow
import tech.xdge.muxa.protocol.WorkItem
import tech.xdge.muxa.protocol.WorkspaceInfo
import tech.xdge.muxa.session.PaneSession
import tech.xdge.muxa.session.pickable
import tech.xdge.muxa.session.planIsNew
import tech.xdge.muxa.session.rowOf
import tech.xdge.muxa.session.shown

// The overall assistant's plans, pins, deadlines and notes (ADR 0049). The host ranks and writes;
// a new plan never changes the selection, the scroll or any order here.

/** The host has plans, pins and notes (`work:10`). */
val AppModel.canPlan: Boolean get() = isConnected && state.canPlan

/** Today shows: the host has an overall assistant, or kept plans. */
val AppModel.hasToday: Boolean get() = canPlan && (state.assistant?.overall != null || state.plans.isNotEmpty())

/** The plan whose rows Today shows. */
val AppModel.shownPlan: Plan? get() = if (canPlan) state.plans.shown() else null

/** The shown plan is newer than the one the user saw. */
val AppModel.planIsNew: Boolean get() = planIsNew(shownPlan, seenPlan)

/**
 * The first plans this device lists count as seen; later ones are New until a row is opened. With no
 * plan yet, the empty id is what was seen, so the first plan made is New too.
 */
internal fun AppModel.notePlans() {
    if (seenPlan == null && canPlan) seenPlan = shownPlan?.id ?: ""
}

/** Today's row of [item], when the shown plan lists it. */
fun AppModel.todayRow(item: WorkItem): PlanRow? = shownPlan?.rowOf(item.id)

/** A row opens its Work item or Inbox item; the plan counts as seen. */
fun AppModel.openPlanRow(row: PlanRow) {
    seenPlan = shownPlan?.id
    row.work?.let { workDetail = it }
    row.item?.let { openItem(it) }
}

/** The New Task and Capture pickers' workspaces: not the overall assistant's (ADR 0049 §2). */
val AppModel.pickableWorkspaces: List<WorkspaceInfo> get() = state.tasks?.workspaces.orEmpty().pickable(state.assistant)

/** Plan now, with the time the user has. Returns the host's reason when it refused. */
suspend fun AppModel.planNow(minutes: Int?): String? = try {
    session.planNow(minutes)
    null
} catch (e: PaneSession.Failure) {
    PaneSession.describe(e)
}

/** Sets the deadline. Returns the host's reason when it refused. */
suspend fun AppModel.setDue(item: WorkItem, due: Due): String? = try {
    session.setDue(item, due)
    null
} catch (e: PaneSession.Failure) {
    PaneSession.describe(e)
}

/** The key [appliedNotes] keeps for [result]'s notes: each step of a version proposes its own (ADR 0051 §5). */
fun notesKey(request: AssistantRequest, result: AssistResult) = "${request.id}:${result.version}:${result.step ?: 1}"

/**
 * Update notes: writes [result]'s proposal to its live notes file, the workspace's or, for a step's
 * staff member, the member's own. Returns the host's reason when it could not.
 */
suspend fun AppModel.applyNotes(request: AssistantRequest, result: AssistResult): String? {
    val notes = result.notes ?: return null
    return try {
        session.applyNotes(request.id, result.version, notes, result.step)
        appliedNotes = appliedNotes + notesKey(request, result)
        null
    } catch (e: PaneSession.Failure) {
        PaneSession.describe(e)
    }
}
