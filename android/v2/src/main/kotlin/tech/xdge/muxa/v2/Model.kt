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

/** A place a client asks for (ADR 0068). A repo is absolute, or relative to the Workspace's root. */
sealed interface PlaceSpec {
    /** A new folder in the Workspace's tasks folder. */
    data object Folder : PlaceSpec

    /** A worktree of [repo] on [branch]. */
    data class Branch(val repo: String, val branch: String, val base: String? = null) : PlaceSpec

    /** The repo's main checkout, never written. */
    data class Read(val repo: String) : PlaceSpec
}

/** A Task's place as it is, from its agent's `muxa.places` label; the working place comes first. */
@Serializable
data class Place(val kind: PlaceKind, val path: String, val branch: String? = null)

@Serializable
enum class PlaceKind {
    @SerialName("folder") FOLDER,
    @SerialName("branch") BRANCH,
    @SerialName("read") READ,
}

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

    /** A Task's places, the working place first; empty for other terminals. */
    val places: List<Place>
        get() = labels["muxa.places"]?.let { runCatching { json.decodeFromString<List<Place>>(it) }.getOrNull() } ?: emptyList()
}

/** What the Reader lists and opens (plan F2). */
enum class ReaderKind(val wire: String) { DOCS("docs"), CHANGES("changes"), IMAGES("images") }

/** Which diff of a changed file. */
enum class DiffScope(val wire: String) {
    /** Against HEAD: staged, unstaged and untracked. */
    UNCOMMITTED("uncommitted"),

    /** The branch's commits since it left its base. */
    BRANCH("branch"),
}

/** A file of a terminal's folder the Reader may open. Which fields are set depends on the kind. */
@Serializable
data class ReaderEntry(
    val path: String,
    /** Documents: an HTML `<title>`. */
    val title: String? = null,
    val mtime: Long? = null,
    /** Images: bytes. */
    val size: Long? = null,
    /** Changes: `modified`, `added`, `deleted`, `renamed`, `untracked`, `conflicted`. */
    val status: String? = null,
    @SerialName("old_path") val oldPath: String? = null,
    val added: Int? = null,
    val removed: Int? = null,
)

@Serializable
data class ReaderBranch(val base: String, val commits: Int, val entries: List<ReaderEntry>, val truncated: Boolean = false)

@Serializable
data class ReaderList(
    /** The repo's folder name; null outside a repo. */
    val root: String? = null,
    val entries: List<ReaderEntry>,
    val truncated: Boolean = false,
    /** Changes: what the branch's commits changed. */
    val branch: ReaderBranch? = null,
)

/** An open file as it is now. */
@Serializable
data class ReaderContent(
    val path: String,
    /** Documents and diffs. */
    val text: String? = null,
    /** Images: the bytes in base64. */
    val data: String? = null,
    /** A diff of a binary file. */
    val binary: Boolean = false,
    val mtime: Long? = null,
) {
    /** An image's bytes. */
    val bytes: ByteArray? get() = data?.let { runCatching { java.util.Base64.getDecoder().decode(it) }.getOrNull() }
}

/** A Task Loop a client asks for (ADR 0019). */
@Serializable
data class LoopSpec(
    /** Null: the Workspace's only allowed check. */
    val check: String? = null,
    /** 1 to 10; null means 3. */
    @SerialName("max_attempts") val maxAttempts: Int? = null,
    /** Seconds one check may run; null means 600. */
    val timeout: Long? = null,
)

/** Where a Task's loop stands. Times are Unix seconds. */
@Serializable
data class LoopInfo(
    val terminal: String,
    val check: String,
    @SerialName("max_attempts") val maxAttempts: Int,
    val timeout: Long,
    /** `armed`, `checking`, `paused`, `passed` or `stopped`. */
    val state: String,
    val attempts: Int,
    @SerialName("armed_at") val armedAt: Long,
    val since: Long,
    /** `waiting`, `limit`, `error`, `no-agent`, `not-sent`, `user`… */
    val reason: String? = null,
    @SerialName("last_check") val lastCheck: LastCheck? = null,
) {
    /** It still works on the Task. */
    val isActive: Boolean get() = state in setOf("armed", "checking", "paused")
}

@Serializable
data class LastCheck(
    val attempt: Int,
    /** Null when the check timed out. */
    val exit: Int? = null,
    val seconds: Long,
    val at: Long,
)

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

    /** A Task Loop changed; every loop as it is now. */
    data class Loops(val loops: List<LoopInfo>) : V2Event

    /** An open Reader file changed. */
    data class Reader(val reader: Int, val content: ReaderContent) : V2Event

    /** An open Reader file ended; the reader is closed. */
    data class ReaderEnded(val reader: Int, val reason: String) : V2Event

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
