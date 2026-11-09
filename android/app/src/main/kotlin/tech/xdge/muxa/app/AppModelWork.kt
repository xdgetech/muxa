package tech.xdge.muxa.app

import kotlinx.coroutines.launch
import tech.xdge.muxa.protocol.AssistScope
import tech.xdge.muxa.protocol.AssistantRequest
import tech.xdge.muxa.protocol.Capture
import tech.xdge.muxa.protocol.Session
import tech.xdge.muxa.protocol.SortAction
import tech.xdge.muxa.protocol.WorkAction
import tech.xdge.muxa.protocol.WorkItem
import tech.xdge.muxa.session.CaptureDraft
import tech.xdge.muxa.session.PaneSession
import tech.xdge.muxa.session.TaskRecordDraft
import tech.xdge.muxa.session.UnsentCapture
import tech.xdge.muxa.session.assistNote
import tech.xdge.muxa.session.forWork
import tech.xdge.muxa.session.listed
import tech.xdge.muxa.session.unsentReason

// Captures and Work items (ADR 0046 §10). Every change is written on the host at the `rev` this
// app saw; a failure lands in `inboxError` with the host's reason, and the Inbox is read again.

/** The host keeps Captures and Work items, so `+` opens Capture and the Inbox has three sections. */
val AppModel.canCapture: Boolean get() = isConnected && state.canCapture

/** The Inbox's Work section, in the host's order. */
val AppModel.listedWork: List<WorkItem> get() = if (canCapture) state.work.listed() else emptyList()

/** The Inbox's Unsorted section: the host's unsorted Captures. */
val AppModel.unsortedCaptures: List<Capture> get() = if (canCapture) state.captures else emptyList()

/** This host's captures that wait on the device, oldest first. */
val AppModel.unsentHere: List<UnsentCapture> get() = unsentList.filter { it.host == currentHostId }

fun AppModel.workItem(id: String): WorkItem? = state.work.firstOrNull { it.id == id }

/** The assistant requests of the listed Work items. */
val AppModel.listedRequests: List<AssistantRequest> get() = if (canCapture) state.requests else emptyList()

/** The Work row's note on its newest request; null without one. */
fun AppModel.assistNote(item: WorkItem): String? = assistNote(item, listedRequests)

/** `+` (ADR 0035 §5, changed by ADR 0046): Capture on a host with `work:8`, else New Task. */
fun AppModel.openPlus() {
    if (!canCapture) return startNewTask()
    // The workspaces come with the task list.
    if (state.tasks == null) scope.launch { runCatching { session.loadTasks() } }
    captureDraft = CaptureDraft()
}

/**
 * Saves the capture. When the connection is down it stays on the device as "Not sent" and is
 * resent on connect with the same `clientId`. Throws only the host's refusal, for the sheet.
 */
suspend fun AppModel.saveCapture(draft: CaptureDraft) {
    if (draft.problem != null) return
    val text = draft.text.trim()
    try {
        session.capture(draft.clientId, text, draft.workspace)
    } catch (e: PaneSession.Failure) {
        if (unsentReason(e) != null) throw e
        val host = currentHostId ?: throw e
        unsent.add(UnsentCapture(draft.clientId, host, text, draft.workspace, System.currentTimeMillis() / 1000))
        captureNotice = NOT_SENT
    }
    captureDraft = null
}

/**
 * Ask Assistant on the Capture sheet: saves with `assist`. When the assistant cannot start, the host
 * still keeps the Capture; the sheet closes and the notice gives the host's reason. Without a
 * connection it waits as "Not sent", and is sent later without asking: nothing starts by itself.
 */
suspend fun AppModel.askFromCapture(draft: CaptureDraft) {
    if (draft.askProblem != null) return
    val text = draft.text.trim()
    val workspace = draft.workspace ?: return
    val assisted = try {
        session.captureAndAsk(draft.clientId, text, workspace, draft.assist)
    } catch (e: PaneSession.Failure) {
        if (unsentReason(e) != null) throw e
        val host = currentHostId ?: throw e
        unsent.add(UnsentCapture(draft.clientId, host, text, workspace, System.currentTimeMillis() / 1000))
        captureNotice = NOT_SENT_ASK
        captureDraft = null
        return
    }
    captureDraft = null
    val refusal = assisted.refusal ?: return
    val where = if (assisted.work != null) "as a Work item" else "under Unsorted"
    assistNotice = "Saved $where. The assistant did not start: ${PaneSession.describe(refusal)}"
}

/** Reads the item with its Captures' texts for the detail. Returns the host's reason when it could not. */
suspend fun AppModel.loadWorkDetail(id: String): String? = try {
    session.getWork(id)
    null
} catch (e: PaneSession.Failure) {
    PaneSession.describe(e)
}

/** Ask Assistant or Ask again on the detail. Returns the host's reason when it refused. */
suspend fun AppModel.askAssistant(item: WorkItem, scope: AssistScope, purpose: String?, workspace: String?): String? =
    busyWith(item.id) { session.requestAssistant(item, scope, purpose, workspace) }

/** Cancel on a request whose run has no result yet. Returns the host's reason when it refused. */
suspend fun AppModel.cancelRequest(item: WorkItem, request: AssistantRequest): String? =
    busyWith(item.id) { session.cancelRequest(request) }

/** The session of [request]'s run, while it lives. */
fun AppModel.runSession(request: AssistantRequest): Session? = request.run?.let { run -> state.sessions.firstOrNull { it.taskId == run } }

/** Open Session: the run's pane, as a Sessions row opens it. */
fun AppModel.openRunSession(session: Session) {
    workDetail = null
    open(session)
}

private suspend fun AppModel.busyWith(id: String, action: suspend () -> Unit): String? {
    if (changingWork != null) return null
    changingWork = id
    return try {
        action()
        null
    } catch (e: PaneSession.Failure) {
        PaneSession.describe(e)
    } finally {
        changingWork = null
    }
}

/** "Start a Task instead…": the New Task form with the text, and no Capture saved. */
fun AppModel.captureToTask(draft: CaptureDraft) {
    captureDraft = null
    showRecordSheet { draft.taskDraft(it) }
}

/** Sends this host's unsent captures, oldest first; stops when the connection drops again. */
fun AppModel.resendUnsent() = scope.launch {
    if (!state.canCapture) return@launch
    for (c in unsentHere) if (!resend(c)) return@launch
}

/** Send Now on a "Not sent" row. */
fun AppModel.sendNow(c: UnsentCapture) = scope.launch { resend(c) }

/** False when the connection is down. A refusal stays on the row with the host's reason. */
private suspend fun AppModel.resend(c: UnsentCapture): Boolean {
    try {
        session.capture(c.clientId, c.text, c.workspace)
        unsent.remove(c.clientId)
    } catch (e: PaneSession.Failure) {
        val reason = unsentReason(e) ?: return false
        unsent.refused(c.clientId, reason)
    }
    return true
}

fun AppModel.sort(capture: Capture, action: SortAction, into: WorkItem? = null) = change(capture.id) {
    session.sortCapture(capture, action, into)
}

/** Complete, Later, Reopen or Withdraw. Complete and Withdraw close the detail. */
fun AppModel.changeWork(item: WorkItem, action: WorkAction) = change(item.id) {
    session.updateWork(item, action)
    if (action == WorkAction.COMPLETE || action == WorkAction.WITHDRAW) workDetail = null
}

/** "Start a Task": the New Task form for [item]; the host makes one Task per item `rev`. */
fun AppModel.startWorkTask(item: WorkItem) {
    workDetail = null
    showRecordSheet { TaskRecordDraft.forWork(item, it) }
}

/** One change at a time per row; a stale `rev` says the item changed (PaneSession.WORK_CHANGED). */
private fun AppModel.change(id: String, action: suspend () -> Unit) {
    if (changingWork != null) return
    changingWork = id
    scope.launch {
        try {
            action()
        } catch (e: PaneSession.Failure) {
            inboxError = PaneSession.describe(e)
        } finally {
            changingWork = null
        }
    }
}

/** What the app says when Ask Assistant could not reach the host. */
const val NOT_SENT_ASK = "Not sent yet. Muxa keeps it on this device and sends it when it connects to the host. " +
    "The assistant is not asked then: ask from the Inbox once it is there."

/** What the app says when a Save could not reach the host. */
const val NOT_SENT = "Not sent yet. Muxa keeps it on this device and sends it when it connects to the host."
