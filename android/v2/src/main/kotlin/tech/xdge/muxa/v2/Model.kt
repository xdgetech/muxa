package tech.xdge.muxa.v2

import kotlinx.serialization.KSerializer
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.SerializationException
import kotlinx.serialization.descriptors.SerialDescriptor
import kotlinx.serialization.encoding.Decoder
import kotlinx.serialization.encoding.Encoder
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonDecoder
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonEncoder
import kotlinx.serialization.json.JsonPrimitive

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
    /** `capture`, `task`, `routine` or `v1`. */
    val source: String,
    /** The Task's agent terminal this item is about. */
    val terminal: String? = null,
    /** The Routine whose run made this item. */
    val routine: RoutineRef? = null,
    /** Where a v1 Task worked: Resume Task adopts it as a [PlaceSpec.Existing] place. */
    val place: ItemPlace? = null,
) {
    fun isLater(now: Long) = (laterUntil ?: 0) > now
}

@Serializable
data class RoutineRef(val workspace: String, val name: String)

/** A working place that already exists in [workspace]. */
@Serializable
data class ItemPlace(val workspace: String, val path: String)

/** What happens to a Task's branch worktree when its Review item is approved. */
enum class WorktreeChoice(val wire: String) {
    REMOVE("remove"),

    /** Remove it even with uncommitted or unmerged work. */
    FORCE("force"),
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

/** A Workspace whose Routines are in its file but turned off: none run. */
@Serializable
data class RoutinesOff(val workspace: String, val count: Int)

/** A Workspace as the host defines it, for starting a Task. */
@Serializable
data class Workspace(
    val name: String,
    val root: String,
    val repos: List<String>,
    val agent: String? = null,
    /** Null: the Workspace has no Task Loop. */
    val loop: WorkspaceLoop? = null,
    val connections: List<WorkspaceConnection> = emptyList(),
    /** The MCP servers a Task can name (ADR 0069). */
    val mcp: List<WorkspaceMcp> = emptyList(),
)

@Serializable
data class WorkspaceLoop(val checks: LoopChecks)

/** The check commands a Task Loop may run: any, or only the listed ones. */
@Serializable(with = LoopChecksSerializer::class)
sealed interface LoopChecks {
    data object Any : LoopChecks
    data class List(val commands: kotlin.collections.List<String>) : LoopChecks
}

/** `"any"` or an array of commands. */
internal object LoopChecksSerializer : KSerializer<LoopChecks> {
    private const val ANY = "any"

    override val descriptor: SerialDescriptor = JsonElement.serializer().descriptor

    override fun serialize(encoder: Encoder, value: LoopChecks) {
        val out = encoder as? JsonEncoder ?: throw SerializationException("loop checks are JSON only")
        out.encodeJsonElement(
            when (value) {
                LoopChecks.Any -> JsonPrimitive(ANY)
                is LoopChecks.List -> JsonArray(value.commands.map(::JsonPrimitive))
            },
        )
    }

    override fun deserialize(decoder: Decoder): LoopChecks {
        val input = decoder as? JsonDecoder ?: throw SerializationException("loop checks are JSON only")
        return when (val e = input.decodeJsonElement()) {
            is JsonPrimitive if e.isString && e.content == ANY -> LoopChecks.Any
            is JsonArray -> LoopChecks.List(e.map(::command))
            else -> throw SerializationException("checks are \"$ANY\" or a list of commands")
        }
    }

    private fun command(e: JsonElement): String =
        (e as? JsonPrimitive)?.takeIf { it.isString }?.content ?: throw SerializationException("a check is a string")
}

/**
 * A connection of a Workspace; default ones go to every Task (ADR 0069). [lastUsed] is the Unix
 * second an agent last started with it; null: never.
 */
@Serializable
data class WorkspaceConnection(
    val name: String,
    /** Always true since ADR 0075: every agent of the Workspace gets it. */
    @SerialName("default") val isDefault: Boolean,
    @SerialName("used_by") val usedBy: UsedBy = UsedBy(),
    @SerialName("last_used") val lastUsed: Long? = null,
)

/** An MCP server of a Workspace, by name only; every agent of the Workspace gets it (ADR 0075). */
@Serializable
data class WorkspaceMcp(
    val name: String,
    @SerialName("default") val isDefault: Boolean,
    @SerialName("used_by") val usedBy: UsedBy = UsedBy(),
    @SerialName("last_used") val lastUsed: Long? = null,
)

/**
 * Who gets a connection or MCP server now (ADR 0069 §7): the [routines] in `routines.toml`, by
 * name, and the live Task agents, by terminal id. A default one counts for all of them.
 */
@Serializable
data class UsedBy(val routines: List<String> = emptyList(), val tasks: List<String> = emptyList())

/** A place a client asks for (ADR 0068). A repo is absolute, or relative to the Workspace's root. */
sealed interface PlaceSpec {
    /** A new folder in the Workspace's tasks folder. */
    data object Folder : PlaceSpec

    /** A worktree of [repo] on [branch]. */
    data class Branch(val repo: String, val branch: String, val base: String? = null) : PlaceSpec

    /** The repo's main checkout, never written. */
    data class Read(val repo: String) : PlaceSpec

    /** A folder or worktree that already exists: nothing is made. It is the only working place. */
    data class Existing(val path: String) : PlaceSpec
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
    /** A Task's title as the user wrote it; the name is only a slug of it. */
    val taskTitle: String? get() = labels["muxa.title"]
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

/** One item of an agent's conversation (plan F4). */
@Serializable
data class ChatEntry(
    val id: String,
    /** `user`, `assistant`, `tool` or `notice`. */
    val kind: String,
    val text: String? = null,
    val tool: ChatTool? = null,
)

@Serializable
data class ChatTool(val name: String, val summary: String, val result: String? = null, val error: Boolean = false)

/** What the agent waits on, with the input that picks each option. */
@Serializable
data class ChatPrompt(
    val entry: String,
    /** `permission`, `question` or `plan`. */
    val kind: String,
    val question: String? = null,
    val options: List<ChatOption>,
)

@Serializable
data class ChatOption(
    val label: String,
    /** Sent as the terminal's input to pick this option. */
    val input: String,
    val cancel: Boolean = false,
)

/** What changed in a followed conversation. */
@Serializable
data class ChatUpdate(
    /** [entries] replace everything; else each replaces the entry with its id, or is added. */
    val reset: Boolean = false,
    val entries: List<ChatEntry>,
    val truncated: Boolean = false,
    /** What the agent waits on now; null: nothing known. */
    val prompt: ChatPrompt? = null,
) {
    /** [before] after this update. */
    fun appliedTo(before: List<ChatEntry>): List<ChatEntry> {
        if (reset) return entries
        val out = before.toMutableList()
        for (e in entries) {
            val at = out.indexOfFirst { it.id == e.id }
            if (at >= 0) out[at] = e else out += e
        }
        return out
    }
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

    /** A followed conversation changed. */
    data class Chat(val chat: Int, val update: ChatUpdate) : V2Event

    /** A followed conversation ended; the chat is closed. */
    data class ChatEnded(val chat: Int, val reason: String) : V2Event

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
