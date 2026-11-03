package tech.xdge.muxa.protocol

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

/** The shared capability of the newer features (ADR 0027): the host sends `work:<level>`. */
const val WORK_CAPABILITY = "work"

/** Level 1: Routines and stored Inbox entries (ADR 0026). */
const val WORK_ROUTINES = 1

/** Level 2: repos relative to the workspace root, a branch from the title, and `start-session` (ADR 0031). */
const val WORK_ASK = 2

/** The highest `work` level in [capabilities]; 0 when the host has none. */
fun workLevel(capabilities: List<String>): Int =
    capabilities.mapNotNull { it.removePrefix("$WORK_CAPABILITY:").takeIf { rest -> rest != it }?.toIntOrNull() }
        .maxOrNull() ?: 0

/** A Routine from the host's `workspaces.toml`, with its schedule state. `(workspace, name)` is its id. */
@Serializable
data class RoutineInfo(
    val workspace: String,
    val name: String,
    /** Null: the Routine runs only on `run-routine`. */
    val trigger: RoutineTrigger? = null,
    val enabled: Boolean,
    val worker: String? = null,
    val project: String? = null,
    /** Epoch seconds. */
    val nextDue: Long? = null,
    val lastSuccess: RoutineSuccess? = null,
    val lastFailure: RoutineFailure? = null,
    val running: RoutineRun? = null,
)

/** `HH:MM`, 24-hour, local to the IANA [timezone]. */
@Serializable
data class RoutineTrigger(val at: String, val timezone: String)

/** The open run of a Routine. Its session carries `taskId` = [task]. */
@Serializable
data class RoutineRun(val occurrence: String, val startedAt: Long, val task: String? = null)

@Serializable
data class RoutineSuccess(val at: Long, val occurrence: String, val task: String, val published: Int)

@Serializable
data class RoutineFailure(val at: Long, val occurrence: String, val task: String? = null, val reason: String)

/** A stored Inbox entry: a Routine's briefing or proposal. */
@Serializable
data class InboxEntry(
    val id: String,
    val workspace: String,
    val routine: String,
    val key: String,
    /** `briefing` or `proposal`; a newer host may send another kind. */
    val kind: String,
    val title: String,
    val body: String,
    val links: List<SourceLink> = emptyList(),
    val createdAt: Long,
    /** The `routine-run` record that published it. */
    val run: String,
    val occurrence: String,
    /** `open`, `later`, `approved` or `dismissed`. */
    val disposition: String,
    /** With `later`: epoch seconds. */
    val until: Long? = null,
    val proposal: Proposal? = null,
    /** After approval: the Task that performs it. */
    val task: String? = null,
)

@Serializable
data class SourceLink(val url: String, val title: String? = null)

/** The action an approval starts; [hash] is `sha256:<hex>`. */
@Serializable
data class Proposal(val action: String, val target: String, val content: String, val hash: String)

@Serializable
enum class InboxAction {
    @SerialName("approve") APPROVE,
    @SerialName("dismiss") DISMISS,
    @SerialName("later") LATER,
}
