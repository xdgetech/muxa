package tech.xdge.muxa.protocol

import kotlinx.serialization.Serializable

// Today rows say what to do (ADR 0059), at `work` level 17. The verbs, the primary action and the
// receiver of Send are the client's; the host adds the secretary's Send and the runs left today.

/** Level 17: `start-session` with `text` and `work`, and `assistant.overallRuns` (ADR 0059). */
const val WORK_TODAY_ACTIONS = 17

/** The overall workspace's runs today, for "N of M runs left today". */
@Serializable
data class RunBudget(
    /** Runs a plan may still start today: the smaller of what the overall workspace's and the host's `max_runs_per_day` leave. */
    val left: Int,
    /** The overall workspace's `max_runs_per_day`. */
    val max: Int,
)
