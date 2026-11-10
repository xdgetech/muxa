package tech.xdge.muxa.session

import tech.xdge.muxa.protocol.AssistResult
import tech.xdge.muxa.protocol.AssistantInfo
import tech.xdge.muxa.protocol.AssistantRequest
import tech.xdge.muxa.protocol.Basis
import tech.xdge.muxa.protocol.BasisKind
import tech.xdge.muxa.protocol.Delivery
import tech.xdge.muxa.protocol.Due
import tech.xdge.muxa.protocol.Minutes
import tech.xdge.muxa.protocol.Plan
import tech.xdge.muxa.protocol.PlanRow
import tech.xdge.muxa.protocol.WorkItem
import tech.xdge.muxa.protocol.WorkspaceInfo
import java.time.DateTimeException
import java.time.Instant
import java.time.LocalDate
import java.time.LocalTime
import java.time.ZoneId
import java.time.ZonedDateTime
import java.time.format.DateTimeFormatter
import java.util.Locale

// The overall assistant's plans, pins and deadlines as the clients show them (ADR 0049 §5, §6).
// The host ranks and derives; this only picks the plan to show and its words.

/** The plan whose rows Today shows: the newest `resulted` one. */
fun List<Plan>.shown(): Plan? = firstOrNull { it.delivery == Delivery.RESULTED }

/** A plan run is open: Plan now waits for it. */
fun List<Plan>.planning(): Boolean = firstOrNull()?.delivery in OPEN_PLAN

/**
 * What Today says about the newest plan when its rows are not the ones shown: being made, or
 * ended without rows, with the host's reason as it is. Null when the newest plan is shown.
 */
fun planStatus(plans: List<Plan>): String? {
    val newest = plans.firstOrNull() ?: return null
    return when (newest.delivery) {
        Delivery.RESULTED -> null
        in OPEN_PLAN -> "Making a new plan…"
        else -> listOfNotNull("The newest plan ${newest.delivery.label}", newest.reason).joinToString(": ")
    }
}

/** [shown] is newer than the plan the user [seen]; nothing is new before the first one is seen. */
fun planIsNew(shown: Plan?, seen: String?): Boolean = shown != null && seen != null && shown.id != seen

/** The row's Work item or Inbox item id. */
val PlanRow.target: String? get() = work ?: item

/** "60–120 min", or "5 min" when the estimate has no range. */
val Minutes.label: String get() = if (low == high) "$low min" else "$low–$high min"

/** One reason of a row: its kind, the text, and where it comes from. An unknown kind keeps its name. */
val Basis.line: String
    get() {
        val kindLabel = when (kind) {
            BasisKind.DUE -> "Due"
            BasisKind.DEPENDENCY -> "Depends on"
            BasisKind.GOAL -> "Goal"
            BasisKind.TIME -> "Time"
            BasisKind.MANUAL -> "Your pin"
            else -> kind.raw
        }
        return listOfNotNull("$kindLabel: $text", source).joinToString(" · ")
    }

/** The row of [plan] that names [id], a Work item or an Inbox item. */
fun Plan.rowOf(id: String): PlanRow? = top.firstOrNull { it.target == id }

/** What the host derived about [item]'s requests on [plan]: no answer yet, or a Result older than the item. */
fun Plan.marks(item: WorkItem): List<String> = listOfNotNull(
    NO_ANSWER.takeIf { item.requests.any { it in unanswered } },
    OUT_OF_DATE.takeIf { item.requests.any { it in stale } },
)

const val NO_ANSWER = "no answer yet"
const val OUT_OF_DATE = "result out of date"

/** "Due soon, moved down by you: Renew the domain, …", with [titles] by id. Null when nothing is due soon. */
fun Plan.dueSoonLine(titles: (String) -> String): String? =
    dueSoon.takeIf { it.isNotEmpty() }?.joinToString(", ", prefix = "Due soon, moved down by you: ") { titles(it) }

val WorkItem.isPinned: Boolean get() = pinnedAt != null

/** The workspaces a New Task or Capture picker offers: the overall assistant's is left out (ADR 0049 §2). */
fun List<WorkspaceInfo>.pickable(assistant: AssistantInfo?): List<WorkspaceInfo> =
    assistant?.overall?.let { overall -> filterNot { it.name == overall } } ?: this

/** A delegate is named after its workspace. */
fun delegateName(workspace: String): String = "$workspace's assistant"

/**
 * The newest current Result among [requests] when it proposes notes, with its request: the Notes
 * section's source. Null when the newest Result has none.
 */
fun proposedNotes(requests: List<AssistantRequest>): Pair<AssistantRequest, AssistResult>? {
    val newest = requests.mapNotNull { q -> q.currentResults.firstOrNull()?.let { q to it } }.maxByOrNull { it.second.at }
    return newest?.takeIf { it.second.notes != null }
}

/** "due Wed 12 Nov 16:00", in the deadline's own zone, with the zone when it is not [here]. */
fun Due.label(here: ZoneId = ZoneId.systemDefault()): String {
    val zone = runCatching { ZoneId.of(timezone) }.getOrDefault(here)
    val at = Instant.ofEpochSecond(this.at).atZone(zone)
    val text = "due ${at.format(DUE_FORMAT)}"
    return if (zone.normalized() == here.normalized()) text else "$text ${zone.id}"
}

/** What the Due sheet edits: a date and time in [zone], an IANA name that defaults to the device's. */
data class DueDraft(val date: LocalDate, val time: LocalTime, val zone: String) {
    /** What to fix before Set; null when it can be sent. */
    val problem: String?
        get() = when {
            zoneId == null -> "Use a time zone name such as Asia/Tokyo."
            epochSeconds() <= 0 -> "Pick a later date."
            else -> null
        }

    private val zoneId: ZoneId? get() = if (zone.contains('/') || zone == "UTC") runCatching { ZoneId.of(zone.trim()) }.getOrNull() else null

    private fun epochSeconds(): Long = zoneId?.let { ZonedDateTime.of(date, time, it).toEpochSecond() } ?: 0

    /** The deadline to send; null while [problem] is set. */
    fun toDue(): Due? = if (problem != null) null else Due(epochSeconds(), zone.trim())

    companion object {
        /** The item's deadline, or tomorrow at 17:00 in [here]. */
        fun of(due: Due?, here: ZoneId = ZoneId.systemDefault(), today: LocalDate = LocalDate.now(here)): DueDraft {
            if (due == null) return DueDraft(today.plusDays(1), LocalTime.of(DEFAULT_HOUR, 0), here.id)
            val zone = try {
                ZoneId.of(due.timezone)
            } catch (e: DateTimeException) {
                here
            }
            val at = Instant.ofEpochSecond(due.at).atZone(zone)
            return DueDraft(at.toLocalDate(), at.toLocalTime().withSecond(0), zone.id)
        }

        private const val DEFAULT_HOUR = 17
    }
}

private val DUE_FORMAT = DateTimeFormatter.ofPattern("EEE d MMM HH:mm", Locale.ENGLISH)
private val OPEN_PLAN = setOf(Delivery.PENDING, Delivery.RECEIVED, Delivery.PROCESSING)
