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
            // A Task's shells are listed with their Task, never on their own.
            val live = terminals.filter { it.live && !it.isTaskShell }
            val (tasks, others) = live.partition { it.isTask }
            return TerminalGroups(tasks, others)
        }
    }
}

/** The live shells of the Task whose agent is [agent] (ADR 0079), in the server's order. */
fun List<Terminal>.shellsOf(agent: Terminal): List<Terminal> {
    val dir = agent.taskDir ?: return emptyList()
    if (!agent.isTaskAgent) return emptyList()
    return filter { it.live && it.isTaskShell && it.taskDir == dir }
}

/**
 * A terminal muxa started for a Task. A Routine run also carries `muxa.dir`,
 * so the kind decides: Routine runs never mix with Tasks.
 */
val Terminal.isTask: Boolean get() = labels["muxa.kind"] == TASK_KIND && taskDir != null

/** What a terminal is called in lists: a Task's title, branch or name, a Routine run's Routine, else its id. */
val Terminal.title: String
    get() = taskTitle?.ifEmpty { null } ?: branch ?: name ?: labels["muxa.routine"]?.let { "Routine $it" } ?: "Terminal $id"

private const val TASK_KIND = "task"
