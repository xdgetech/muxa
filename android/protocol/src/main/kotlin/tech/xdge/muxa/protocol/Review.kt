package tech.xdge.muxa.protocol

import kotlinx.serialization.Serializable

// A request's Result to review (ADR 0058), at `work` level 16. Every stored Result makes one
// `result` Inbox item, answered like a Task's review: hand off (`approve`), send the member what
// to do next (`reply`), done (`dismiss`), or later.

/** Level 16: `result` Inbox items, and `reply` on `result` and `write` items (ADR 0058). */
const val WORK_RESULT_REVIEW = 16

/** A `result` item's Result, so the user reviews it without opening the Work item. Its summary is the item's `summary`. Never logged. */
@Serializable
data class ResultReview(
    /** `q-…`. */
    val request: String,
    val version: Long,
    /** The step of [version] that produced the Result; 1 without a staff. */
    val step: Int,
    /** The Work item, to open its detail. */
    val work: String,
    /** The staff member who wrote it. Null: the workspace has no staff. */
    val worker: String? = null,
    /** Each finding's first line, at most 200 characters. */
    val findings: List<String> = emptyList(),
    val openQuestions: List<String> = emptyList(),
    val next: List<String> = emptyList(),
    /** Paths the run wrote, absolute. */
    val files: List<String> = emptyList(),
    /** The hand-off the step proposes; `approve` starts it while it is offered. */
    val handoff: Handoff? = null,
    /** Level 18: the Task the version proposes; Start Task starts it (ADR 0060 §2). */
    val task: TaskProposal? = null,
)
