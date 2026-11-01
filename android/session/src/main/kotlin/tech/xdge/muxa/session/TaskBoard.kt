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

    companion object {
        fun of(list: Message.TaskList) = TaskBoard(list.workspaces, list.entries, list.truncated)
    }
}
