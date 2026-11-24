package tech.xdge.muxa.v2

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

/** A subscription, a named agent login, and the plan limits its agents last reported (ADR 0091). No credential and no account. */
@Serializable
data class SubscriptionUsage(
    val name: String,
    /** `claude-code` or `codex`. */
    val agent: String,
    /** A label only. */
    val plan: String? = null,
    /** Shortest first; empty before its first report. */
    val windows: List<UsageWindow> = emptyList(),
    /** Unix seconds of the last report; null before the first. */
    @SerialName("as_of") val asOf: Long? = null,
)

/** One plan window, such as 5 hours (300 minutes) or a week (10080). */
@Serializable
data class UsageWindow(
    val minutes: Int,
    /** 0 to 100. */
    @SerialName("used_percent") val usedPercent: Int,
    /** Unix seconds. */
    @SerialName("resets_at") val resetsAt: Long,
) {
    /** The window reset after its last report: its percent is no longer known. */
    fun hasReset(nowSeconds: Long = System.currentTimeMillis() / 1000): Boolean = resetsAt <= nowSeconds
}
