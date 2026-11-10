package tech.xdge.muxa.protocol

import kotlinx.serialization.Serializable

/** A Task: a durable record of work, kept by the bridge on the host. It outlives the session that runs it (ADR 0015). */
@Serializable
data class TaskRecord(
    /** e.g. `t-7f3a2c`. */
    val id: String,
    /** Grows by one on every write; `update-task` sends the one the client saw. */
    val rev: Long,
    val workspace: String,
    val title: String,
    val goal: String,
    val status: TaskStatus,
    /** Epoch seconds. */
    val createdAt: Long,
    /** Epoch seconds. */
    val updatedAt: Long,
    val project: String? = null,
    /** The repo's main checkout, absolute. */
    val repo: String? = null,
    val branch: String? = null,
    /** Where the agent works: the worktree or the task directory, absolute. */
    val dir: String? = null,
    /** The last known session name, for display. Sessions link by `taskId`. */
    val session: String? = null,
    /** From the bridge, e.g. why the launch failed. */
    val note: String? = null,
    /** The agents that worked on the task, oldest first (ADR 0018). Recorded only while a bridge started or restarted them. */
    val runs: List<AgentRun> = emptyList(),
    /** The task's Task Loop; null when it has none (ADR 0019). */
    val loop: TaskLoop? = null,
    /** A start or restart prompt the host still waits to type (ADR 0019). */
    val pendingPrompt: PendingPrompt? = null,
    val worker: String? = null,
    val restartOperation: RestartOperation? = null,
    /** The Work item the Task was started for (ADR 0048). */
    val work: String? = null,
)

/** One agent that worked on a task, as the bridge started or restarted it (ADR 0018). */
@Serializable
data class AgentRun(
    /** From 1, one higher than the previous run's. */
    val id: Long,
    /** The adapter that saw the agent, e.g. `claude-code`; null until one did. */
    val agent: String? = null,
    /** Epoch seconds. */
    val startedAt: Long,
    /** Epoch seconds when the bridge restarted it; null otherwise. */
    val endedAt: Long? = null,
    /** The agent's own session id. */
    val agentSession: String? = null,
    /** The session's transcript on the host, absolute. A path only. */
    val transcript: String? = null,
    val reason: RunReason,
    /** The worker requested for this run, or `default`; null on a legacy run means unrecorded. */
    val worker: String? = null,
    val outcome: String? = null,
)

/** Why the bridge started a run. A reason added by a newer bridge keeps its name. */
@Serializable
@JvmInline
value class RunReason(val raw: String) {
    companion object {
        /** `create-task` started the agent. */
        val START = RunReason("start")

        /** `restart-agent` replaced the agent. */
        val RESTART = RunReason("restart")
    }
}

/**
 * Set by the user; separate from the agent's state. A passing Task Loop moves `open` to `review` (ADR 0019).
 * A status added by a newer bridge keeps its name.
 */
@Serializable
@JvmInline
value class TaskStatus(val raw: String) {
    companion object {
        val OPEN = TaskStatus("open")
        val REVIEW = TaskStatus("review")
        val DONE = TaskStatus("done")
        val DROPPED = TaskStatus("dropped")
    }
}

/** A workspace from the host's config; `create-task` names one. */
@Serializable
data class WorkspaceInfo(
    val name: String,
    /** The check commands a Task Loop may run here; null means no loops. */
    val checks: WorkspaceChecks? = null,
    /** Its main checkouts, absolute and sorted: the repos under its root and those the config lists. */
    val repos: List<String> = emptyList(),
    /** The root folder, absolute (ADR 0031); null from older hosts. A repo under it shows by its relative path. */
    val root: String? = null,
    /** What a Task's end does with its "For the workspace" (`work` level 14, ADR 0053); null from older hosts. */
    val memory: MemoryMode? = null,
    /** Epoch seconds of the last change to `.muxa/WORKSPACE.md`; null when it does not exist. */
    val memoryAt: Long? = null,
)
