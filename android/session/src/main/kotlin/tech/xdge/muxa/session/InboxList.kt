package tech.xdge.muxa.session

import tech.xdge.muxa.protocol.AgentState
import tech.xdge.muxa.protocol.InboxEntry
import tech.xdge.muxa.protocol.RoutineInfo
import tech.xdge.muxa.protocol.Session
import java.time.Instant
import java.time.ZoneId

/**
 * A Routine run whose agent waits on the user. The run is not a Task, but its need
 * shows in Inbox (ADR 0026 section 3). [task] is the run's record id: the session's `taskId`.
 */
data class RoutineNeed(val routine: RoutineInfo, val task: String, val session: Session)

/** One row of the Inbox list: a Task need, a stored entry, or a waiting Routine run. */
sealed interface InboxRow {
    val id: String

    data class Task(val item: TaskItem) : InboxRow {
        override val id get() = item.record.id
    }

    data class Entry(val entry: InboxEntry) : InboxRow {
        override val id get() = entry.id
    }

    data class Routine(val need: RoutineNeed) : InboxRow {
        override val id get() = need.task
    }
}

val InboxEntry.isBriefing: Boolean get() = kind == "briefing"
val InboxEntry.isProposal: Boolean get() = kind == "proposal" && proposal != null

/** Still waits for the user: open, or back from Later ([now], epoch seconds). An unknown disposition shows without actions. */
fun InboxEntry.isShown(now: Long): Boolean = when (disposition) {
    "open" -> true
    "later" -> (until ?: 0) <= now
    "approved", "dismissed" -> false
    else -> true
}

/** The host knows the kind and the disposition, so actions can be offered. */
val InboxEntry.hasKnownActions: Boolean
    get() = (isBriefing || isProposal) && (disposition == "open" || disposition == "later")

/** `(workspace, name)` identifies a Routine. */
val RoutineInfo.key: String get() = "$workspace/$name"

/** Runs whose agent waits on the user, matched by `running.task` to a session's `taskId`. */
fun List<RoutineInfo>.needs(sessions: List<Session>): List<RoutineNeed> = mapNotNull { routine ->
    val task = routine.running?.task ?: return@mapNotNull null
    val session = sessions.firstOrNull { it.taskId == task } ?: return@mapNotNull null
    val waits = session.panes.any { !it.shellForeground && it.agent?.state == AgentState.WAITING }
    if (waits) RoutineNeed(routine, task, session) else null
}

/**
 * The Inbox as one ordered list (ADR 0025, "List first"): Task needs in the host's queue
 * order, waiting Routine runs, the stored entries newest first, and last the Tasks moved
 * down, so Move down puts a Task at the tail of the whole list. [queue] is [triageQueue],
 * already without hidden Tasks. [now] is epoch seconds.
 */
fun inboxRows(
    queue: List<TaskItem>,
    entries: List<InboxEntry>,
    routines: List<RoutineInfo>,
    sessions: List<Session>,
    now: Long = System.currentTimeMillis() / 1000,
): List<InboxRow> {
    val (moved, waiting) = queue.partition { it.place?.order != null }
    val runs = routines.needs(sessions).map(InboxRow::Routine)
    val stored = entries.filter { it.isShown(now) }.map(InboxRow::Entry)
    return waiting.map(InboxRow::Task) + runs + stored + moved.map(InboxRow::Task)
}

/** The times Later offers. Each is in the future, as the host requires. */
enum class LaterChoice(val title: String) {
    IN_AN_HOUR("In 1 hour"),
    TOMORROW_MORNING("Tomorrow 9:00");

    /** Epoch seconds. Tomorrow is the next day in [zone]. */
    fun until(now: Instant = Instant.now(), zone: ZoneId = ZoneId.systemDefault()): Long = when (this) {
        IN_AN_HOUR -> now.epochSecond + 3_600
        TOMORROW_MORNING -> now.atZone(zone).toLocalDate().plusDays(1).atTime(9, 0).atZone(zone).toEpochSecond()
    }
}

/** Why the host would refuse [until] (epoch seconds): in the past, or more than 366 days ahead. Null when it is fine. */
fun laterProblem(until: Long, now: Long = System.currentTimeMillis() / 1000): String? = when {
    until <= now -> "Pick a time in the future."
    until - now > LATER_LIMIT_SECONDS -> "Pick a time within a year."
    else -> null
}

private const val LATER_LIMIT_SECONDS = 366L * 86_400
