package tech.xdge.muxa.v2

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json

/** What an Inbox item asks of the user (ADR 0066). */
@Serializable
enum class ItemKind {
    @SerialName("review") REVIEW,
    @SerialName("reply") REPLY,
    @SerialName("decide") DECIDE,
    @SerialName("read") READ,
    @SerialName("todo") TODO,
    @SerialName("failed") FAILED,
    ;

    val wire: String get() = name.lowercase()
}

@Serializable
data class InboxItem(
    val id: String,
    val kind: ItemKind,
    val title: String,
    val note: String? = null,
    val workspace: String? = null,
    /** `YYYY-MM-DD`. */
    val due: String? = null,
    /** Hidden until then (Unix seconds). */
    @SerialName("later_until") val laterUntil: Long? = null,
    val created: Long,
    /** `capture`, `task` or `routine`. */
    val source: String,
) {
    fun isLater(now: Long) = (laterUntil ?: 0) > now
}

/** Open items in the user's order, then recent done ones. */
@Serializable
data class Inbox(val revision: Long, val items: List<InboxItem>, val done: List<InboxItem>) {
    companion object {
        val EMPTY = Inbox(0, emptyList(), emptyList())
    }
}

/** A Capture line read into fields, shown as chips before the item is added. */
@Serializable
data class Capture(val kind: ItemKind, val title: String, val workspace: String? = null, val due: String? = null)

/** A Routine as the host defines it, with its last run. `workspace/name` is its id. */
@Serializable
data class Routine(
    val workspace: String,
    val name: String,
    val enabled: Boolean,
    /** `HH:MM` in [timezone]; null for a Routine that runs only on request. */
    val at: String? = null,
    val timezone: String? = null,
    /** The five cron fields; null means every day. */
    val schedule: String? = null,
    @SerialName("next_due") val nextDue: Long? = null,
    @SerialName("running_since") val runningSince: Long? = null,
    val last: RoutineLast? = null,
) {
    val id: String get() = "$workspace/$name"
}

@Serializable
data class RoutineLast(val at: Long, val ok: Boolean, val reason: String? = null, val published: Int? = null)

@Serializable
data class RoutineProblem(val workspace: String, val reason: String)

@Serializable
data class TerminalStatus(val state: String, val app: String? = null)

/** A terminal's facts; never its screen. */
@Serializable
data class Terminal(
    val id: String,
    val name: String? = null,
    val live: Boolean,
    val status: TerminalStatus? = null,
    val labels: Map<String, String> = emptyMap(),
    val cols: Int,
    val rows: Int,
) {
    val taskDir: String? get() = labels["muxa.dir"]
    val branch: String? get() = labels["muxa.branch"]
    val isTaskAgent: Boolean get() = labels["muxa.kind"] == "task" && labels["muxa.role"] == "agent"
}

@Serializable
data class Exit(val reason: String, @SerialName("exit_code") val exitCode: Long? = null, val signal: Long? = null)

/** What the server sends besides replies. */
sealed interface V2Event {
    /** Output to show; bytes the client already had are cut off. */
    class Output(val channel: Int, val bytes: ByteArray) : V2Event {
        override fun equals(other: Any?) = other is Output && channel == other.channel && bytes.contentEquals(other.bytes)
        override fun hashCode() = 31 * channel + bytes.contentHashCode()
        override fun toString() = "Output($channel, ${bytes.size} bytes)"
    }

    /** The next output starts a fresh screen. */
    data class Paint(val channel: Int) : V2Event

    /** The channel is over: the exit, or null when detached or not resumed. */
    data class Closed(val channel: Int, val exit: Exit?) : V2Event
    data class InboxChanged(val inbox: Inbox) : V2Event
    data class Changed(val terminal: Terminal) : V2Event
    data class Removed(val terminal: String) : V2Event

    /** Changes were lost: list the terminals again. */
    data object Resync : V2Event

    /** The connection is gone; resume with [V2Client.state]. */
    data class Lost(val reason: String) : V2Event
}

sealed class V2Error(message: String) : Exception(message) {
    /** The server answered with an error. */
    class Refused(val code: String, message: String) : V2Error("$message ($code)")
    class Lost(reason: String) : V2Error(reason)
    class BadMessage(message: String) : V2Error(message)
}

internal val json = Json {
    ignoreUnknownKeys = true
    explicitNulls = false
}
