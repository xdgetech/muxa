package tech.xdge.muxa.app

import kotlinx.coroutines.launch
import tech.xdge.muxa.protocol.InboxItem
import tech.xdge.muxa.protocol.MemoryProposal
import tech.xdge.muxa.protocol.RoutineInfo
import tech.xdge.muxa.protocol.Session
import tech.xdge.muxa.protocol.WriteProposal
import tech.xdge.muxa.session.PaneSession
import tech.xdge.muxa.session.defaultPane
import tech.xdge.muxa.session.key

/** What an Inbox action asks the user before it goes to the host. */
sealed interface InboxPrompt {
    /** A proposal item's action, target and content, before approval. */
    data class ApproveItem(val item: InboxItem) : InboxPrompt

    /** A `write` item's target once more, before Approve sends [write]'s hash (ADR 0052 §7). */
    data class ApproveWrite(val item: InboxItem, val write: WriteProposal) : InboxPrompt

    /** A `memory` item's workspace once more, before Approve sends [memory]'s hash (ADR 0053 §7). */
    data class ApproveMemory(val item: InboxItem, val memory: MemoryProposal) : InboxPrompt

    /** Reject on the overall assistant's item, with an optional reason the host records (ADR 0050 §2). */
    data class RejectItem(val item: InboxItem) : InboxPrompt

    /** Delete Old's confirmation: [count] listed items older than [days], not protected (ADR 0046). */
    data class DeleteOld(val days: Int, val count: Int) : InboxPrompt
}

// The Routines' actions (ADR 0026). A failure lands in `inboxError` with the host's reason.

fun AppModel.routine(key: String): RoutineInfo? = state.routines.firstOrNull { it.key == key }

/** The session of a Routine's open run, by its `taskId`. */
fun AppModel.runSession(routine: RoutineInfo): Session? {
    val task = routine.running?.task ?: return null
    return state.sessions.firstOrNull { it.taskId == task && it.defaultPane != null }
}

/** Starts a run of [routine] now; a refusal shows the host's message. */
fun AppModel.runNow(routine: RoutineInfo, done: () -> Unit = {}) = perform {
    try {
        session.runRoutine(routine)
    } finally {
        done()
    }
}

private fun AppModel.perform(action: suspend () -> Unit) = scope.launch {
    try {
        action()
    } catch (e: PaneSession.Failure) {
        inboxError = PaneSession.describe(e)
    }
}
