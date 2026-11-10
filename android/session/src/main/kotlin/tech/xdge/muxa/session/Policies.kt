package tech.xdge.muxa.session

import tech.xdge.muxa.protocol.Basis
import tech.xdge.muxa.protocol.BasisKind
import tech.xdge.muxa.protocol.Confidence
import tech.xdge.muxa.protocol.Correction
import tech.xdge.muxa.protocol.CorrectionKind
import tech.xdge.muxa.protocol.Hypothesis
import tech.xdge.muxa.protocol.HypothesisState
import tech.xdge.muxa.protocol.InboxItem
import tech.xdge.muxa.protocol.ItemKind
import tech.xdge.muxa.protocol.Plan
import tech.xdge.muxa.protocol.PlanRow
import tech.xdge.muxa.protocol.Policy
import java.time.DayOfWeek
import java.time.Instant
import java.time.LocalDate
import java.time.LocalTime
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import java.time.temporal.TemporalAdjusters
import java.util.Locale

// What the user told the overall assistant, and what the host saw (ADR 0050), as the clients show
// it. The host keeps and checks every record; this only orders Today and finds the words.

/** Today's rows in the user's order when the plan has one (`userOrder`), else the agent's. */
val Plan.todayRows: List<PlanRow>
    get() {
        if (userOrder.isEmpty()) return top
        val byTarget = top.associateBy { it.target }
        val ordered = userOrder.mapNotNull { byTarget[it] }
        return ordered + top.filter { it !in ordered }
    }

/** The user removed the row from Today ("Not today"). */
fun Plan.isDismissed(row: PlanRow): Boolean = row.target in dismissed

/** The `order` that moves [target] by [offset] rows in [todayRows]; null when it cannot move. */
fun Plan.moving(target: String, offset: Int): List<String>? {
    val order = todayRows.mapNotNull { it.target }
    val from = order.indexOf(target)
    val to = from + offset
    if (from < 0 || to !in order.indices) return null
    return order.toMutableList().apply { add(to, removeAt(from)) }
}

/** The record a `policy`, `hypothesis` or `correction` basis cites; [Cited.Forgotten] when it is gone. */
sealed interface Cited {
    data class PolicyRecord(val policy: Policy) : Cited
    data class HypothesisRecord(val hypothesis: Hypothesis) : Cited
    data class CorrectionRecord(val correction: Correction) : Cited

    /** The id names no record any more: forgotten, deleted, or expired out of the lists. */
    data class Forgotten(val id: String) : Cited
}

/** What the user remembers on the host, from `inbox-list` (`work` level 11). */
data class Memory(
    val policies: List<Policy> = emptyList(),
    val hypotheses: List<Hypothesis> = emptyList(),
    val corrections: List<Correction> = emptyList(),
) {
    /** The cited record of [basis]; null for the other kinds, which cite no record. */
    fun cited(basis: Basis): Cited? {
        val id = basis.source ?: return null
        return when (basis.kind) {
            BasisKind.POLICY -> policies.firstOrNull { it.id == id }?.let(Cited::PolicyRecord)
            BasisKind.HYPOTHESIS -> hypotheses.firstOrNull { it.id == id }?.let(Cited::HypothesisRecord)
            BasisKind.CORRECTION -> corrections.firstOrNull { it.id == id }?.let(Cited::CorrectionRecord)
            else -> return null
        } ?: Cited.Forgotten(id)
    }
}

/** The open `hypothesis` item that asks about [hypothesis], whose answers confirm or reject it. */
fun List<InboxItem>.askingAbout(hypothesis: Hypothesis): InboxItem? =
    firstOrNull { it.kind == ItemKind.HYPOTHESIS && it.hypothesis?.hypothesis == hypothesis.id && it.isWaiting(Long.MAX_VALUE) }

/** The policy is past its `until`; it stays listed for 30 days and is in no brief. */
fun Policy.isExpired(now: Long): Boolean = until?.let { it <= now } == true

/** "All workspaces", or the workspace's name. */
fun scopeLabel(scope: String): String = if (scope == ALL_SCOPE) "All workspaces" else scope

/** "All workspaces · from Mon 10 Nov · until Sun 16 Nov", with "expired" past its end. */
fun Policy.span(now: Long, zone: ZoneId = ZoneId.systemDefault()): String = listOfNotNull(
    scopeLabel(scope),
    from?.let { "from ${day(it, zone)}" },
    until?.let { "until ${day(it, zone)}" } ?: "no end",
    "expired".takeIf { isExpired(now) },
).joinToString(" · ")

/** "Based on 3 corrections": the evidence, by count only. */
val Hypothesis.basedOn: String get() = basedOn(evidence.size)

fun basedOn(count: Int): String = if (count == 1) "Based on 1 correction" else "Based on $count corrections"

val Confidence.label: String
    get() = when (this) {
        Confidence.LOW -> "low confidence"
        Confidence.MEDIUM -> "medium confidence"
        else -> raw
    }

val HypothesisState.label: String
    get() = when (this) {
        HypothesisState.PROPOSED -> "proposed"
        HypothesisState.REJECTED -> "rejected"
        HypothesisState.CONFIRMED -> "confirmed"
        HypothesisState.EXPIRED -> "expired"
        else -> raw
    }

/** Forget is offered for a `proposed`, `rejected` or `expired` hypothesis; a confirmed one is its policy now. */
val Hypothesis.canForget: Boolean get() = state != HypothesisState.CONFIRMED

/** "Not today: Renew the domain — “waiting on the registrar”", with [titles] by row id. */
fun Correction.summary(titles: (String) -> String): String {
    val title = row?.let(titles)
    val what = when (kind) {
        CorrectionKind.REORDERED -> "You reordered Today"
        CorrectionKind.DISMISSED -> "Not today: $title"
        CorrectionKind.REJECTED -> "You rejected: $title"
        CorrectionKind.PINNED -> "You pinned: $title"
        CorrectionKind.UNPINNED -> "You unpinned: $title"
        CorrectionKind.DEFERRED -> "Later: $title"
        else -> listOfNotNull(kind.raw, title).joinToString(": ")
    }
    return reason?.let { "$what — “$it”" } ?: what
}

/** The policy text limit: 500 bytes, one paragraph (PROTOCOL.md). */
const val MAX_POLICY_BYTES = 500

/** The `scope` of a policy for every workspace. */
const val ALL_SCOPE = "all"

/** When a policy ends: never, at this week's Sunday 23:59:59, or at the end of a day. */
sealed interface PolicyEnd {
    data object None : PolicyEnd
    data object ThisWeek : PolicyEnd
    data class Until(val date: LocalDate) : PolicyEnd
}

/** What the Policy sheet edits. [from] starts the policy at that day's start; null means now. */
data class PolicyDraft(
    val text: String = "",
    val scope: String = ALL_SCOPE,
    val from: LocalDate? = null,
    val end: PolicyEnd = PolicyEnd.None,
) {
    /** What to fix before Save; null when it can be sent. */
    fun problem(now: Long, zone: ZoneId = ZoneId.systemDefault()): String? {
        val bytes = text.trim().toByteArray().size
        val until = until(now, zone)
        return when {
            bytes == 0 -> "Write the policy in your own words."
            bytes > MAX_POLICY_BYTES -> "Keep the policy under $MAX_POLICY_BYTES bytes."
            text.trim().any { it.isISOControl() } -> "Write the policy as one paragraph, without line breaks."
            until != null && until <= now -> "Pick an end after today."
            until != null && from(zone)?.let { until <= it } == true -> "Pick an end after the start."
            else -> null
        }
    }

    /** Epoch seconds at the start of [from] in [zone]. */
    fun from(zone: ZoneId = ZoneId.systemDefault()): Long? = from?.atStartOfDay(zone)?.toEpochSecond()

    /** Epoch seconds at the end of the last day, as the CLI's `--until week` counts it. */
    fun until(now: Long, zone: ZoneId = ZoneId.systemDefault()): Long? = when (end) {
        PolicyEnd.None -> null
        PolicyEnd.ThisWeek -> endOfDay(Instant.ofEpochSecond(now).atZone(zone).toLocalDate().with(TemporalAdjusters.nextOrSame(DayOfWeek.SUNDAY)), zone)
        is PolicyEnd.Until -> endOfDay(end.date, zone)
    }

    companion object {
        /** [policy] as it is, for Edit; its end as a day. */
        fun of(policy: Policy, zone: ZoneId = ZoneId.systemDefault()) = PolicyDraft(
            policy.text,
            policy.scope,
            policy.from?.let { Instant.ofEpochSecond(it).atZone(zone).toLocalDate() },
            policy.until?.let { PolicyEnd.Until(Instant.ofEpochSecond(it).atZone(zone).toLocalDate()) } ?: PolicyEnd.None,
        )

        private fun endOfDay(date: LocalDate, zone: ZoneId): Long = date.atTime(LocalTime.of(23, 59, 59)).atZone(zone).toEpochSecond()
    }
}

private fun day(epoch: Long, zone: ZoneId): String = Instant.ofEpochSecond(epoch).atZone(zone).format(DAY_FORMAT)

private val DAY_FORMAT = DateTimeFormatter.ofPattern("EEE d MMM", Locale.ENGLISH)
