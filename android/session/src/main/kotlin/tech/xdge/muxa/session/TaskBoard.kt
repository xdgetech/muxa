package tech.xdge.muxa.session

import tech.xdge.muxa.protocol.Message
import tech.xdge.muxa.protocol.TaskRecord
import tech.xdge.muxa.protocol.WorkspaceInfo

/** The host's task records and workspaces, as last listed (capability `task-records`). */
data class TaskBoard(
    val workspaces: List<WorkspaceInfo> = emptyList(),
    val records: List<TaskRecord> = emptyList(),
    /** The bridge left some records out. */
    val truncated: Boolean = false,
) {
    fun record(id: String): TaskRecord? = records.firstOrNull { it.id == id }

    /** With [saved] in place of the record it replaces, or added when it is new. A newer listed rev wins. */
    fun saving(saved: TaskRecord): TaskBoard {
        val known = records.any { it.id == saved.id }
        if (!known) return copy(records = records + saved)
        return copy(records = records.map { if (it.id == saved.id && saved.rev >= it.rev) saved else it })
    }

    companion object {
        fun of(list: Message.TaskList) = TaskBoard(list.workspaces, list.entries, list.truncated)
    }
}
