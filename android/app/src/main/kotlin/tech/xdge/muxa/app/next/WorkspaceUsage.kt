package tech.xdge.muxa.app.next

import tech.xdge.muxa.v2.LoopChecks
import tech.xdge.muxa.v2.Terminal
import tech.xdge.muxa.v2.UsedBy
import tech.xdge.muxa.v2.WorkspaceLoop

/** Who uses a connection or MCP server: Routines by name, Tasks by their title. */
data class Users(val routines: List<String>, val tasks: List<String>) {
    val isEmpty: Boolean get() = routines.isEmpty() && tasks.isEmpty()
}

/** [UsedBy] with each Task's terminal id replaced by its title from [terminals], or the id when it is not listed. */
fun UsedBy.named(terminals: List<Terminal>): Users =
    Users(routines, tasks.map { id -> terminals.firstOrNull { it.id == id }?.title ?: id })

/** "never", "just now", "5m ago", "2h ago", "3d ago", "4mo ago" or "1y ago". [lastUsed] and [now] are Unix seconds. */
fun lastUsedText(lastUsed: Long?, now: Long): String {
    if (lastUsed == null) return "never"
    val ago = (now - lastUsed).coerceAtLeast(0)
    return when {
        ago < MINUTE -> "just now"
        ago < HOUR -> "${ago / MINUTE}m ago"
        ago < DAY -> "${ago / HOUR}h ago"
        ago < MONTH -> "${ago / DAY}d ago"
        ago < YEAR -> "${ago / MONTH}mo ago"
        else -> "${ago / YEAR}y ago"
    }
}

/** A Workspace's Task Loop: not allowed, any check, or the checks it may run. */
fun loopText(loop: WorkspaceLoop?): String = when (val checks = loop?.checks) {
    null -> "Not allowed"
    LoopChecks.Any -> "Allowed, any check"
    is LoopChecks.List -> "Allowed: " + checks.commands.joinToString(", ")
}

private const val MINUTE = 60L
private const val HOUR = 60 * MINUTE
private const val DAY = 24 * HOUR
private const val MONTH = 30 * DAY
private const val YEAR = 365 * DAY
