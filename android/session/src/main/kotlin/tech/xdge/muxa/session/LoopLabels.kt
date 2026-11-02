package tech.xdge.muxa.session

import tech.xdge.muxa.protocol.LoopCheck
import tech.xdge.muxa.protocol.LoopReason
import tech.xdge.muxa.protocol.LoopState
import tech.xdge.muxa.protocol.TaskLoop

/*
 * The words the apps use for a Task Loop (ADR 0019). Labels match the iOS app. A state or
 * reason from a newer bridge keeps its raw name: no meaning is invented for it (invariant 6).
 */

/** Shown where the bridge has no `task-loop`: the controls are hidden (ADR 0019). */
const val LOOP_NEEDS_DAEMON = "The Task Loop needs the muxa daemon. On the host, run: muxa-bridge daemon install"

/** The loop still acts on its own: it can be stopped, not started. */
val TaskLoop.isActive: Boolean
    get() = state == LoopState.ARMED || state == LoopState.CHECKING || state == LoopState.PAUSED

/** A state this app knows the meaning of. */
val TaskLoop.isKnownState: Boolean
    get() = isActive || state == LoopState.PASSED || state == LoopState.STOPPED

fun loopStateTitle(state: LoopState): String = when (state) {
    LoopState.ARMED -> "Armed"
    LoopState.CHECKING -> "Checking"
    LoopState.PAUSED -> "Paused"
    LoopState.PASSED -> "Passed"
    LoopState.STOPPED -> "Stopped"
    else -> state.raw
}

/** One plain sentence: what the loop does now, or why it paused or stopped. */
fun loopExplanation(loop: TaskLoop): String = when (loop.state) {
    LoopState.ARMED -> "Runs the check after the agent's next turn."
    LoopState.CHECKING -> "The check is running."
    LoopState.PASSED -> loop.lastCheck?.let { "The check passed on attempt ${it.attempt}." } ?: "The check passed."
    LoopState.PAUSED -> when (loop.reason) {
        LoopReason.WAITING -> "The agent waits for you. The loop resumes after the agent finishes its turn."
        else -> "Paused: ${loop.reason?.raw ?: "no reason given"}."
    }
    LoopState.STOPPED -> stopExplanation(loop)
    else -> "The host reports the state \"${loop.state.raw}\"."
}

private fun stopExplanation(loop: TaskLoop): String = when (loop.reason) {
    LoopReason.LIMIT -> "Stopped: the check failed ${loop.maxAttempts} times."
    LoopReason.TIMEOUT -> "Stopped: a check ran longer than ${checkDuration(loop.timeout)}."
    LoopReason.ERROR -> "Stopped: the agent reported an error."
    LoopReason.NO_AGENT -> "Stopped: no agent runs in this Task's session."
    LoopReason.NOT_SENT -> "Stopped: a prompt could not be typed within 30 seconds."
    LoopReason.STALLED -> "Stopped: the agent did not take the typed prompt."
    LoopReason.STATUS -> "Stopped: the Task's status is no longer Open."
    LoopReason.USER -> "Stopped by the user."
    null -> "Stopped."
    else -> "Stopped: ${loop.reason?.raw}."
}

/** "2/3": failed checks so far, of the most allowed. */
fun loopAttempts(loop: TaskLoop): String = "${loop.attempts}/${loop.maxAttempts}"

/** The Inbox row's badge: "Loop 2/3", "Loop checking", "Loop paused", "Loop passed", "Loop stopped: limit". */
fun loopBadge(loop: TaskLoop): String = when (loop.state) {
    LoopState.ARMED -> "Loop ${loopAttempts(loop)}"
    LoopState.CHECKING -> "Loop checking"
    LoopState.PAUSED -> "Loop paused"
    LoopState.PASSED -> "Loop passed"
    LoopState.STOPPED -> when (val reason = loop.reason) {
        null, LoopReason.USER -> "Loop stopped"
        else -> "Loop stopped: ${reason.raw.replace('-', ' ')}"
    }
    else -> "Loop: ${loop.state.raw}"
}

/** "Attempt 2 · exit 1 · 42 s · 5 min ago", or "timed out" in place of the exit code. */
fun lastCheckLine(check: LoopCheck, now: Long): String {
    val result = check.exit?.let { "exit $it" } ?: "timed out"
    return "Attempt ${check.attempt} · $result · ${checkDuration(check.seconds)} · ${duration(now - check.at)} ago"
}

/** Seconds as "42 s" under a minute, else as [duration] says it. */
fun checkDuration(seconds: Long): String = if (seconds < 60) "${seconds.coerceAtLeast(0)} s" else duration(seconds)
