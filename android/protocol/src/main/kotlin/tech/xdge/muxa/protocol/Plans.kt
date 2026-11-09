package tech.xdge.muxa.protocol

import kotlinx.serialization.Serializable

// Delegate notes, the overall assistant and its plans (ADR 0047), at `work` level 9.
// A plan never changes a Work item, an Inbox item or their order.

/** One overall run's plan, from `plan-saved` or `inbox-list`: at most 3 rows to do next. */
@Serializable
data class Plan(
    /** `p-` and 8 hex digits. */
    val id: String,
    /** Grows by one on every write; `apply-notes` sends it as `version`. */
    val rev: Long,
    /** Epoch seconds: the schedule's time, or when `plan-now` asked. */
    val at: Long,
    /** Epoch seconds: when the host read the inputs for the brief. */
    val inputsAt: Long,
    val delivery: Delivery,
    /** Epoch seconds: when [delivery] was last derived. */
    val deliveryAt: Long,
    /** The time the user has, from `plan-now`. */
    val minutes: Int? = null,
    /** The run record, `t-…` of kind `assistant-run`. */
    val run: String? = null,
    /** The workspaces whose data the brief carried. */
    val workspaces: List<String> = emptyList(),
    /** At most 3, pinned Work items first. */
    val top: List<PlanRow> = emptyList(),
    /** Work items moved down with Later whose `due` is within 24 hours. */
    val dueSoon: List<String> = emptyList(),
    /** Derived when listed: requests whose run has not answered. */
    val unanswered: List<String> = emptyList(),
    /** Derived when listed: requests whose newest result is older than their Work item. */
    val stale: List<String> = emptyList(),
    /** Derived against the previous plan. */
    val changed: List<PlanChange> = emptyList(),
    /** The overall assistant's proposed notes. Never logged. */
    val notes: Notes? = null,
    /** Why a plan is `failed` or `cancelled`: host text naming ids and limits. */
    val reason: String? = null,
)

/** One row of a plan's `top`: a Work item or an Inbox item. */
@Serializable
data class PlanRow(
    val work: String? = null,
    val item: String? = null,
    val why: String,
    val next: String,
    val minutes: Minutes,
    val basis: List<Basis> = emptyList(),
    /** What the agent would need to know to rank it better. */
    val missing: List<String> = emptyList(),
)

/** The agent's estimate of the time a row takes: `low <= estimated <= high`. */
@Serializable
data class Minutes(val low: Int, val high: Int, val estimated: Int)

/** One reason a row is in the plan. [source] is a URL, a path or an id; [at] epoch seconds. */
@Serializable
data class Basis(val kind: BasisKind, val text: String, val source: String? = null, val at: Long? = null)

/** Unknown kinds stay as their name. */
@Serializable
@JvmInline
value class BasisKind(val raw: String) {
    companion object {
        val DUE = BasisKind("due")
        val DEPENDENCY = BasisKind("dependency")
        val GOAL = BasisKind("goal")
        val TIME = BasisKind("time")

        /** The user's pin. */
        val MANUAL = BasisKind("manual")
    }
}

/** A row that entered, left or moved since the previous plan. [id] is the Work item or Inbox item. */
@Serializable
data class PlanChange(val id: String, val change: ChangeKind)

/** Unknown changes stay as their name. */
@Serializable
@JvmInline
value class ChangeKind(val raw: String) {
    companion object {
        val ENTERED = ChangeKind("entered")
        val LEFT = ChangeKind("left")
        val MOVED = ChangeKind("moved")
    }
}

/** A `sort` Inbox item's action: what `sort-capture` would do. [work] goes with `link`. */
@Serializable
data class SortProposal(
    val capture: String,
    val action: SortAction,
    val workspace: String? = null,
    val title: String? = null,
    val work: String? = null,
)

/** A `delegate` Inbox item's action: what `request-assistant` would do in the receiving [workspace]. */
@Serializable
data class DelegateProposal(
    val work: String,
    val workspace: String,
    val scope: AssistScope,
    /** The ask, at most 1 KiB. Never logged. */
    val purpose: String,
    /** Why the overall assistant asks. Never logged. */
    val reason: String,
)

/** Who assists on this host, in `inbox-list`. */
@Serializable
data class AssistantInfo(
    /** The overall assistant's workspace. Leave it out of workspace pickers. */
    val overall: String? = null,
    val delegates: List<DelegateInfo> = emptyList(),
)

/** A workspace with `[workspace.assistant]`. */
@Serializable
data class DelegateInfo(
    val workspace: String,
    val share: Share,
    val overallRequests: OverallRequests,
    /** Epoch seconds: when its `NOTES.md` last changed. Null: no notes. */
    val notesAt: Long? = null,
)

/** What a workspace shares with the overall assistant (ADR 0047 §3). Unknown values stay as their name. */
@Serializable
@JvmInline
value class Share(val raw: String) {
    companion object {
        val NONE = Share("none")
        val OUTLINE = Share("outline")
        val SUMMARIES = Share("summaries")
    }
}

/** Whether the overall assistant may ask this workspace (ADR 0047 §4). Unknown values stay as their name. */
@Serializable
@JvmInline
value class OverallRequests(val raw: String) {
    companion object {
        /** Refuses even approved requests. */
        val NONE = OverallRequests("none")

        /** The user approves each one. */
        val REVIEW = OverallRequests("review")

        /** Sent without approval, within every budget. */
        val AUTO = OverallRequests("auto")
    }
}
