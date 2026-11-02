package tech.xdge.muxa.demo

import tech.xdge.muxa.protocol.LoopCheck
import tech.xdge.muxa.protocol.LoopReason
import tech.xdge.muxa.protocol.LoopSpec
import tech.xdge.muxa.protocol.LoopState
import tech.xdge.muxa.protocol.TaskLoop
import tech.xdge.muxa.protocol.TaskStatus
import tech.xdge.muxa.protocol.WorkspaceChecks
import tech.xdge.muxa.protocol.WorkspaceInfo

/* The demo's Task Loop, as the host's daemon answers it (ADR 0019). It never runs a check. */

/** Why the host refuses [spec] in [workspace], as `bad-request`; null when it allows it. */
internal fun loopRefusal(workspace: WorkspaceInfo?, spec: LoopSpec): String? {
    val name = workspace?.name ?: return "not a workspace"
    val maxAttempts = spec.maxAttempts ?: DEFAULT_MAX_ATTEMPTS
    if (maxAttempts !in 1..10) return "maxAttempts must be 1 to 10"
    return when (val checks = workspace.checks) {
        null -> "The workspace $name has no [workspace.loop] table in workspaces.toml, so it has no loops."
        is WorkspaceChecks.Only -> if (spec.check in checks.commands) null else "The workspace $name does not allow this check."
        WorkspaceChecks.Any ->
            if (spec.check.isBlank() || spec.check.toByteArray().size > 4096 || spec.check.any { it.isISOControl() }) {
                "The check must be at most 4096 bytes, with no control character."
            } else {
                null
            }
    }
}

/** A loop that starts now; the attempts go on from [previous] unless [reset]. */
internal fun startedLoop(previous: TaskLoop?, spec: LoopSpec, reset: Boolean, now: Long) = TaskLoop(
    check = spec.check,
    maxAttempts = spec.maxAttempts ?: DEFAULT_MAX_ATTEMPTS,
    timeout = spec.timeout ?: DEFAULT_TIMEOUT,
    state = LoopState.ARMED,
    attempts = if (reset) 0 else previous?.attempts ?: 0,
    armedAt = now,
    since = now,
    lastCheck = previous?.lastCheck,
)

/** The loop after the user set [status]: anything but `open` stops an active loop (ADR 0019). */
internal fun TaskLoop.afterStatus(status: TaskStatus, now: Long): TaskLoop {
    val active = state == LoopState.ARMED || state == LoopState.CHECKING || state == LoopState.PAUSED
    if (!active || status == TaskStatus.OPEN) return this
    return copy(state = LoopState.STOPPED, reason = LoopReason.STATUS, since = now)
}

internal fun TaskLoop.stopped(now: Long) = copy(state = LoopState.STOPPED, reason = LoopReason.USER, since = now)

/** What the daemon would write into `CHECK.md`: the attempt, the exit code, the duration and the log's tail. The iOS demo writes the same. */
internal fun checkLog(check: LoopCheck) = buildString {
    append("# Check\n\n")
    append("- Attempt: ").append(check.attempt).append("\n")
    append("- Exit: ").append(check.exit?.toString() ?: "timed out").append("\n")
    append("- Duration: ").append(check.seconds).append(" s\n\n")
    append("## Output (last 200 lines)\n\n```\n")
    when (check.exit) {
        0 -> append("test result: ok. 128 passed; 0 failed\n")
        null -> append("running 128 tests\ntest search::rebuild_index has been running for over 60 seconds\n")
        else -> {
            append("running 128 tests\n")
            append("test auth::passkey_round_trip ... FAILED\n\n")
            append("failures:\n\n---- auth::passkey_round_trip stdout ----\n")
            append("assertion failed: response.status == 302\n\n")
            append("test result: FAILED. 127 passed; 1 failed\n")
        }
    }
    append("```\n")
}

private const val DEFAULT_MAX_ATTEMPTS = 3
private const val DEFAULT_TIMEOUT = 600L
