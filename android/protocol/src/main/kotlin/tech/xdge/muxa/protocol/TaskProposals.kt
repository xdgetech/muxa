package tech.xdge.muxa.protocol

import kotlinx.serialization.Serializable

// The assistant proposes Tasks (ADR 0060), at `work` level 18: a staff member's Result or the
// secretary's `propose-task` carries a Task to start, and Start Task in the Inbox starts it in
// plan mode.

/** Level 18: `result.task`, `new-task` items, `start-task` and `propose-task` (ADR 0060). */
const val WORK_TASK_PROPOSALS = 18

/** A Task the assistant proposes. Start Task starts it once, on the workspace's default Task worker. Never logged. */
@Serializable
data class TaskProposal(
    val workspace: String,
    /** A repo of the workspace, as `create-task`'s. Null: the Task gets its own folder under the workspace's `tasks/`. */
    val repo: String? = null,
    /** One line, at most 200 characters. */
    val title: String,
    /** What the Task's agent does; at most 4096 bytes. */
    val goal: String,
    /** The Work item the Task is for (`w-…`). */
    val work: String? = null,
    /** On a `result` item: the step whose Result proposed it. */
    val step: Int? = null,
    /** On a `result` item: the staff member who proposed it. */
    val worker: String? = null,
)
