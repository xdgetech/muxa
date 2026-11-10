package tech.xdge.muxa.protocol

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

// Policies, corrections and hypotheses (ADR 0048), at `work` level 10: what the user
// told the overall assistant, and what the host saw. Their texts are never logged.

/** The user's own words, which outrank every hypothesis and note. */
@Serializable
data class Policy(
    /** `y-` and 8 hex digits. */
    val id: String,
    val rev: Long,
    /** 1 to 500 bytes, one paragraph. */
    val text: String,
    /** `all`, or one workspace's name. */
    val scope: String,
    /** Epoch seconds. */
    val from: Long? = null,
    /** Epoch seconds. Past it the policy is expired and in no brief. */
    val until: Long? = null,
    /** `user`, or the `h-` hypothesis it was confirmed from. */
    val source: String,
    val createdAt: Long,
    val updatedAt: Long,
)

/** A user's action on a plan, as the host saw it. */
@Serializable
data class Correction(
    /** `x-` and 8 hex digits. */
    val id: String,
    val kind: CorrectionKind,
    /** Epoch seconds. */
    val at: Long,
    /** The plan that was current. */
    val plan: String,
    /** The Work item or Inbox item. Null with `reordered`. */
    val row: String? = null,
    /** Whether the plan's `top` listed the row. */
    val inTop: Boolean,
    /** The row's workspace; null for nobody's items. */
    val workspace: String? = null,
    /** With `reordered`: the rows' order before and after. */
    val before: List<String> = emptyList(),
    val after: List<String> = emptyList(),
    /** The user's optional reason, at most 500 bytes. */
    val reason: String? = null,
)

/** Unknown kinds stay as their name. */
@Serializable
@JvmInline
value class CorrectionKind(val raw: String) {
    companion object {
        /** Today's rows dragged into another order. */
        val REORDERED = CorrectionKind("reordered")

        /** A row removed from Today ("Not today"). */
        val DISMISSED = CorrectionKind("dismissed")

        /** A `sort`, `delegate` or `hypothesis` item rejected. */
        val REJECTED = CorrectionKind("rejected")
        val PINNED = CorrectionKind("pinned")
        val UNPINNED = CorrectionKind("unpinned")

        /** Later: "not now", never "unimportant". */
        val DEFERRED = CorrectionKind("deferred")
    }
}

/** The overall assistant's guess at a preference, which the user confirms or rejects. */
@Serializable
data class Hypothesis(
    /** `h-` and 8 hex digits. */
    val id: String,
    /** 1 to 500 bytes. */
    val text: String,
    /** `all`, or one workspace's name. */
    val scope: String,
    /** 1 to 5 correction ids. */
    val evidence: List<String>,
    val confidence: Confidence,
    val state: HypothesisState,
    /** The plan that proposed it. */
    val plan: String,
    /** Epoch seconds: when a `proposed` one expires, or a `rejected` one leaves the brief. */
    val expiresAt: Long,
    /** Epoch seconds: when it was proposed. */
    val at: Long,
)

/** Unknown values stay as their name. */
@Serializable
@JvmInline
value class Confidence(val raw: String) {
    companion object {
        val LOW = Confidence("low")
        val MEDIUM = Confidence("medium")
    }
}

/** Unknown states stay as their name. */
@Serializable
@JvmInline
value class HypothesisState(val raw: String) {
    companion object {
        val PROPOSED = HypothesisState("proposed")

        /** It became a policy. */
        val CONFIRMED = HypothesisState("confirmed")
        val REJECTED = HypothesisState("rejected")

        /** Nobody answered it in 14 days. */
        val EXPIRED = HypothesisState("expired")
    }
}

/** A `hypothesis` Inbox item's payload: what `confirm` makes a policy of. [hypothesis] is the `h-` id. */
@Serializable
data class HypothesisProposal(
    val hypothesis: String,
    val text: String,
    val scope: String,
    val evidence: List<String>,
    val confidence: Confidence,
)

/** What `update-policy` does. */
@Serializable
enum class PolicyAction {
    /** Change `text`, `scope`, `from` or `until`. */
    @SerialName("edit") EDIT,
    @SerialName("delete") DELETE,
}

/** What `update-plan` does to the newest `resulted` plan. */
@Serializable
enum class PlanAction {
    /** Set `userOrder` to `order`. */
    @SerialName("reorder") REORDER,

    /** Add `row` to `dismissed`, with `text` as the reason. */
    @SerialName("dismiss") DISMISS,
}
