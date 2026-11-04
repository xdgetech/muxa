package tech.xdge.muxa.session

import tech.xdge.muxa.protocol.AgentRun
import tech.xdge.muxa.protocol.AgentState
import tech.xdge.muxa.protocol.RunReason
import tech.xdge.muxa.protocol.TaskRecord

/** One agent run as the Task detail shows it. Labels match the iOS app. */
data class RunRow(val run: AgentRun, val title: String, val detail: String)

/**
 * [record]'s runs, newest first. A run without `endedAt` is "still running" only when it is
 * the newest and an adapter reports the Task's agent now ([agentReported]); the bridge sets
 * `endedAt` only at its own restarts, so otherwise the end is unknown (invariant 6).
 */
fun runRows(record: TaskRecord, agentReported: Boolean, now: Long): List<RunRow> {
    val newest = record.runs.maxByOrNull { it.id }?.id
    return record.runs.sortedByDescending { it.id }.map { run ->
        val ended = run.endedAt
        val tail = when {
            run.outcome == "unconfirmed" -> "unconfirmed outcome"
            run.outcome != null -> "unknown outcome"
            ended != null -> duration(ended - run.startedAt)
            run.id == newest && agentReported -> "still running"
            else -> "end not recorded"
        }
        RunRow(
            run,
            title = "Run ${run.id} · ${run.agent ?: "unknown agent"}",
            detail = "${reasonTitle(run.reason)} · ${duration(now - run.startedAt)} ago · $tail",
        )
    }
}

fun reasonTitle(reason: RunReason): String = when (reason) {
    RunReason.START -> "Started"
    RunReason.RESTART -> "Restarted"
    else -> reason.raw
}

/** Seconds to "<1 min", "42 min", "1 h", "1 h 5 min". */
fun duration(seconds: Long): String {
    val s = seconds.coerceAtLeast(0)
    if (s < 60) return "<1 min"
    if (s < 3600) return "${s / 60} min"
    val minutes = (s % 3600) / 60
    return if (minutes == 0L) "${s / 3600} h" else "${s / 3600} h $minutes min"
}

/** The bridge writes `.muxa/HANDOFF.md` only when it restarts the agent (ADR 0018). */
val TaskRecord.hasHandoff: Boolean get() = runs.any { it.reason == RunReason.RESTART }

/** An unobserved replacement may still be working, so stopping it also needs confirmation. */
val TaskItem.restartNeedsForce: Boolean
    get() {
        val state = agentPane?.let { pane -> agent?.shown(pane.shellForeground) }
        return state == AgentState.RUNNING ||
            (state == null && record.runs.maxByOrNull { it.id }?.reason == RunReason.RESTART)
    }
