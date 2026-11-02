package tech.xdge.muxa.session

import tech.xdge.muxa.protocol.LoopSpec
import tech.xdge.muxa.protocol.WorkspaceChecks

/**
 * What the Start loop and New Task sheets edit for a Task Loop (ADR 0019), against the
 * workspace's allowed [checks]. Checked here for quick feedback; the host has the final say.
 */
data class LoopDraft(
    /** The workspace's `checks`; null means the workspace has no loops. */
    val checks: WorkspaceChecks?,
    val check: String = defaultCheck(checks),
    val maxAttempts: Int = DEFAULT_MAX_ATTEMPTS,
) {
    /** What to fix before the loop can start; null when it can. */
    val problem: String?
        get() {
            when (checks) {
                null -> return NO_CHECKS
                is WorkspaceChecks.Only -> if (check !in checks.commands) return "Pick a check."
                WorkspaceChecks.Any -> {
                    val command = check.trim()
                    if (command.isEmpty()) return "Enter a check command."
                    if (command.toByteArray().size > MAX_COMMAND_BYTES) return "The check command is too long."
                    if (command.any { it.isISOControl() }) return "The check command cannot contain control characters."
                }
            }
            if (maxAttempts !in ATTEMPTS) return "Max attempts must be 1 to 10."
            return null
        }

    /** The `loop` of the request; null while there is a problem. A listed check is sent exactly as listed. */
    val spec: LoopSpec?
        get() {
            if (problem != null) return null
            val command = if (checks is WorkspaceChecks.Only) check else check.trim()
            return LoopSpec(command, maxAttempts)
        }

    companion object {
        const val DEFAULT_MAX_ATTEMPTS = 3
        val ATTEMPTS = 1..10
        private const val MAX_COMMAND_BYTES = 4096

        /** Shown where a workspace has no `[workspace.loop]` table. */
        const val NO_CHECKS =
            "This workspace has no checks, so it has no loops. The host allows them in workspaces.toml, under [workspace.loop]."

        private fun defaultCheck(checks: WorkspaceChecks?) = (checks as? WorkspaceChecks.Only)?.commands?.firstOrNull() ?: ""
    }
}
