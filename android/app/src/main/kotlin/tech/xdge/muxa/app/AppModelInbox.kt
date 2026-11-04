package tech.xdge.muxa.app

import kotlinx.coroutines.launch
import tech.xdge.muxa.protocol.InboxChange
import tech.xdge.muxa.protocol.InboxEntry
import tech.xdge.muxa.protocol.InboxItem
import tech.xdge.muxa.protocol.RoutineInfo
import tech.xdge.muxa.protocol.Session
import tech.xdge.muxa.protocol.TaskStatus
import tech.xdge.muxa.session.TaskItem
import tech.xdge.muxa.session.InboxRow
import tech.xdge.muxa.session.PaneSession
import tech.xdge.muxa.session.attentionKey
import tech.xdge.muxa.session.defaultPane
import tech.xdge.muxa.session.key

/** What the Inbox screen's list shows. Routines sit under All Tasks. */
enum class InboxPage { INBOX, TASKS, ROUTINES }

/** What an Inbox action asks the user before it goes to the host. */
sealed interface InboxPrompt {
    /** The Later choices for a row. */
    data class Later(val row: InboxRow) : InboxPrompt

    /** A time picked by hand. */
    data class PickTime(val row: InboxRow) : InboxPrompt

    /** The proposal's action, target and content, before approval. */
    data class Approve(val entry: InboxEntry) : InboxPrompt

    /** The Later choices for an Inbox item (ADR 0033). */
    data class ItemLater(val item: InboxItem) : InboxPrompt

    data class ItemPickTime(val item: InboxItem) : InboxPrompt

    /** A proposal item's action, target and content, before approval. */
    data class ApproveItem(val item: InboxItem) : InboxPrompt
}

// The Inbox list's actions (ADR 0025 "List first", ADR 0027 §4). Every one is written on
// the host; nothing here changes a Task's status unless asked to. A failure lands in
// `inboxError` with the host's reason.

fun AppModel.entry(id: String): InboxEntry? = state.inboxEntries.firstOrNull { it.id == id }

fun AppModel.routine(key: String): RoutineInfo? = state.routines.firstOrNull { it.key == key }

/** The session of a Routine's open run, by its `taskId`. */
fun AppModel.runSession(routine: RoutineInfo): Session? {
    val task = routine.running?.task ?: return null
    return state.sessions.firstOrNull { it.taskId == task && it.defaultPane != null }
}

/** Act: a row opens its detail; a waiting Routine run opens its terminal. */
fun AppModel.act(row: InboxRow) {
    inboxFocus = row.id
    clearInboxSelection()
    when (row) {
        is InboxRow.Task -> selectedTask = row.id
        is InboxRow.Entry -> selectedEntry = row.id
        is InboxRow.Routine -> openTerminal(row.need.session)
    }
}

/** Moves the focus [offset] rows, without wrapping. */
fun AppModel.stepInbox(offset: Int) {
    val rows = inboxRows
    if (rows.isEmpty()) return
    val index = rows.indexOfFirst { it.id == inboxFocus }.coerceAtLeast(0)
    inboxFocus = rows[(index + offset).coerceIn(0, rows.lastIndex)].id
}

/** The focused row, else the first. */
val AppModel.focusedRow: InboxRow? get() = inboxRows.let { rows -> rows.firstOrNull { it.id == inboxFocus } ?: rows.firstOrNull() }

/** Move down: to the end of the list, with its status unchanged. */
fun AppModel.moveDown(row: InboxRow) {
    if (row !is InboxRow.Task) return
    perform { session.place(row.item.record, InboxChange(down = true)) }
}

/** Later: out of the list until [until], epoch seconds. */
fun AppModel.later(row: InboxRow, until: Long) {
    inboxPrompt = null
    when (row) {
        is InboxRow.Task -> perform { session.place(row.item.record, InboxChange(until = until)) }
        is InboxRow.Entry -> perform { session.later(row.entry, until) }
        is InboxRow.Routine -> {}
    }
}

/** Done: a briefing is acknowledged, which dismisses it. A Task is marked Done. */
fun AppModel.done(row: InboxRow) {
    when (row) {
        is InboxRow.Task -> perform { session.updateTask(row.item.record, TaskStatus.DONE) }
        is InboxRow.Entry -> dismiss(row.entry)
        is InboxRow.Routine -> {}
    }
}

/** Dismisses a briefing or a proposal; its detail closes. */
fun AppModel.dismiss(entry: InboxEntry) = perform {
    session.dismiss(entry)
    if (selectedEntry == entry.id) selectedEntry = null
}

/** Approves a proposal and opens the one Task the host created for it. */
fun AppModel.approve(entry: InboxEntry) {
    inboxPrompt = null
    perform {
        val task = session.approve(entry).task
        if (task != null) {
            clearInboxSelection()
            selectedTask = task
        }
    }
}

/** The local Snooze of a host without `task-inbox`. */
fun AppModel.snooze(item: TaskItem) {
    inboxSnoozes = inboxSnoozes + (item.record.id to item.attentionKey)
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
