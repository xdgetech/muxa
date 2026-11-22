package tech.xdge.muxa.app.next

import tech.xdge.muxa.v2.Terminal

/**
 * The live terminals as the Tasks lists show them: muxa's Tasks, and the
 * other terminals apart. A terminal that is not live is left out; a failed
 * agent already has a Failed Inbox item.
 */
data class TerminalGroups(val tasks: List<Terminal>, val others: List<Terminal>) {
    val isEmpty: Boolean get() = tasks.isEmpty() && others.isEmpty()

    /** The Tasks by Workspace, in the order they come; null for Tasks without one. */
    val byWorkspace: Map<String?, List<Terminal>> get() = tasks.groupBy { it.workspace }

    companion object {
        fun of(terminals: List<Terminal>): TerminalGroups {
            val live = terminals.filter { it.live }
            val (tasks, others) = live.partition { it.isTask }
            return TerminalGroups(tasks, others)
        }
    }
}

/**
 * A terminal muxa started for a Task. A Routine run also carries `muxa.dir`,
 * so the kind decides: Routine runs never mix with Tasks.
 */
val Terminal.isTask: Boolean get() = labels["muxa.kind"] == TASK_KIND && taskDir != null

/** What a terminal is called in lists: a Task's branch or name, a Routine run's Routine, else its id. */
val Terminal.title: String
    get() = branch ?: name ?: labels["muxa.routine"]?.let { "Routine $it" } ?: "Terminal $id"

private const val TASK_KIND = "task"
