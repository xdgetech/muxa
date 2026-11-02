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

/** Set by the user; separate from the agent's state. A status added by a newer bridge keeps its name. */
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
data class WorkspaceInfo(val name: String)
