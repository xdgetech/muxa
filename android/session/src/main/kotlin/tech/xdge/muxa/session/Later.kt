package tech.xdge.muxa.session

import java.time.Instant
import java.time.ZoneId

/** The times Later offers. Each is in the future, as the host requires. */
enum class LaterChoice(val title: String) {
    IN_AN_HOUR("In 1 hour"),
    TOMORROW_MORNING("Tomorrow 9:00");

    /** Epoch seconds. Tomorrow is the next day in [zone]. */
    fun until(now: Instant = Instant.now(), zone: ZoneId = ZoneId.systemDefault()): Long = when (this) {
        IN_AN_HOUR -> now.epochSecond + 3_600
        TOMORROW_MORNING -> now.atZone(zone).toLocalDate().plusDays(1).atTime(9, 0).atZone(zone).toEpochSecond()
    }
}

/** Why the host would refuse [until] (epoch seconds): in the past, or more than 366 days ahead. Null when it is fine. */
fun laterProblem(until: Long, now: Long = System.currentTimeMillis() / 1000): String? = when {
    until <= now -> "Pick a time in the future."
    until - now > LATER_LIMIT_SECONDS -> "Pick a time within a year."
    else -> null
}

private const val LATER_LIMIT_SECONDS = 366L * 86_400
