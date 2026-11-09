package tech.xdge.muxa.protocol

import kotlinx.serialization.Serializable

/** Changes when a message changes incompatibly. */
const val PROTOCOL_VERSION = 1

/** Sent by the bridge. [Wire] maps each type to its `type` name. */
sealed interface Message {
    /** The first line the bridge sends, before any request. */
    @Serializable
    data class Hello(
        val protocol: Int,
        val bridge: String,
        val tmux: String,
        val capabilities: List<String>,
    ) : Message

    /** The answer to one request. [id] is null only when the request was unreadable. */
    @Serializable
    data class Result(
        val id: Long? = null,
        val error: ErrorBody? = null,
        val sessions: List<Session>? = null,
    ) : Message

    /** Reset the emulator to [cols] x [rows], then feed [data]. */
    @Serializable
    data class Screen(val watch: Long, val pane: String, val cols: Int, val rows: Int, val data: Bytes) : Message

    /**
     * A pane's scrollback, then its screen: lines [cols] wide, each ending in a newline, with
     * colors as escape sequences (capability `history`). For a separate emulator, not the watch's.
     */
    @Serializable
    data class History(val history: Long, val pane: String, val cols: Int, val data: Bytes, val truncated: Boolean = false) : Message

    /** Feed [data] to the emulator of the current screen. */
    @Serializable
    data class Output(val watch: Long, val pane: String, val data: Bytes) : Message

    /** Sessions, windows or panes changed: send `list` again. */
    @Serializable
    data object SessionsChanged : Message

    /** The watched pane no longer exists. */
    @Serializable
    data class PaneGone(val watch: Long, val pane: String, val reason: String? = null) : Message

    /** New or changed entries of a chat. */
    @Serializable
    data class ChatEntries(
        /** The id of the `chat` request this belongs to. */
        val chat: Long,
        /** Grows by one per message of a chat. */
        val seq: Long,
        /** Clear the list before applying [entries]. */
        val reset: Boolean = false,
        val entries: List<ChatEntry>,
        /** The agent's state now; null when unknown. */
        val status: Agent? = null,
        /** On a reset: the adapter that reads this agent, e.g. `claude-code`. */
        val agent: String? = null,
        /** On a reset: earlier history exists but was not sent. */
        val truncated: Boolean = false,
        /** What the agent waits on now, with its choices; null when none is known. */
        val prompt: ChatPrompt? = null,
    ) : Message

    /** The chat stopped, e.g. the agent exited. */
    @Serializable
    data class ChatEnded(val chat: Long, val reason: String) : Message

    /** The documents for a `docs` request, just before its `result`. */
    @Serializable
    data class DocList(
        val docs: Long,
        /** The repo's top directory name. */
        val root: String? = null,
        val entries: List<DocInfo>,
        val truncated: Boolean = false,
    ) : Message

    /** The whole text of the open document, first and after every change. */
    @Serializable
    data class DocContent(val doc: Long, val path: String, val text: String, val mtime: Long? = null) : Message

    /** The open document is no longer readable. */
    @Serializable
    data class DocEnded(val doc: Long, val reason: String) : Message

    /** The changed files for a `changes` request, just before its `result`. */
    @Serializable
    data class ChangeList(
        val changes: Long,
        val root: String? = null,
        /** Against HEAD: staged, unstaged and untracked. */
        val entries: List<ChangeInfo>,
        val truncated: Boolean = false,
        /** What the branch's commits changed; null on the base itself or without one. */
        val branch: BranchChanges? = null,
    ) : Message

    /** The unified diff of the open file, first and after every change; null text for a binary file. */
    @Serializable
    data class DiffContent(
        val diff: Long,
        val path: String,
        val text: String? = null,
        val binary: Boolean = false,
    ) : Message

    /** The open diff stopped, e.g. the file has no changes any more. */
    @Serializable
    data class DiffEnded(val diff: Long, val reason: String) : Message

    /** The images for an `images` request, just before its `result`. */
    @Serializable
    data class ImageList(
        val images: Long,
        val root: String? = null,
        /** Newest first. */
        val entries: List<ImageInfo>,
        val truncated: Boolean = false,
    ) : Message

    /** The whole file of the open image, first and after every change. */
    @Serializable
    data class ImageContent(val image: Long, val path: String, val data: Bytes, val mtime: Long? = null) : Message

    /** The open image is no longer readable. */
    @Serializable
    data class ImageEnded(val image: Long, val reason: String) : Message

    /** The session a `new-task` made or found, just before its `result`. */
    @Serializable
    data class TaskCreated(val task: Long, val session: String) : Message

    /** The task records for a `list-tasks` request, just before its `result`. */
    @Serializable
    data class TaskList(
        val tasks: Long,
        /** The host's workspaces, from its config. */
        val workspaces: List<WorkspaceInfo>,
        val entries: List<TaskRecord>,
        val truncated: Boolean = false,
        val workers: List<WorkerInfo> = emptyList(),
    ) : Message

    /** The record a task request wrote, just before its `result`. */
    @Serializable
    data class TaskSaved(val task: Long, val record: TaskRecord) : Message

    /** A task record changed on the host: send `list-tasks` again (capability `tasks-changed`). */
    @Serializable
    data object TasksChanged : Message

    /** The Routines for a `list-routines` request, just before its `result`. */
    @Serializable
    data class RoutineList(
        val routines: Long,
        val entries: List<RoutineInfo>,
        /** Workspaces whose routines file is broken (ADR 0036). */
        val problems: List<RoutineProblem> = emptyList(),
    ) : Message

    /** The Routine a `run-routine` started, just before its `result`. */
    @Serializable
    data class RoutineSaved(val routine: Long, val entry: RoutineInfo) : Message

    /**
     * The stored Inbox entries for a `list-inbox` request, newest first. Level 3 also sends [items]: every item,
     * Routine entries included, in the order to show them.
     */
    @Serializable
    data class InboxList(
        val inbox: Long,
        val entries: List<InboxEntry>,
        val items: List<InboxItem> = emptyList(),
        /** Level 8: open Work items first, then the ones moved down with Later, then at most 100 done or withdrawn, newest first. */
        val work: List<WorkItem> = emptyList(),
        /** Level 8: unsorted Captures, newest first. */
        val captures: List<Capture> = emptyList(),
        /** Level 8: the requests of the listed Work items. */
        val requests: List<AssistantRequest> = emptyList(),
    ) : Message

    /** What an `update-inbox` wrote, just before its `result`: the Routine entry, the item, or both for a Routine's item. */
    @Serializable
    data class InboxSaved(val inbox: Long, val entry: InboxEntry? = null, val item: InboxItem? = null) : Message

    /** The session a `start-session` made, just before its `result`. [start] is the request id, [session] the `$n` id. */
    @Serializable
    data class SessionStarted(val start: Long, val session: String, val name: String) : Message

    /** What a `capture` or `sort-capture` wrote, just before its `result`. */
    @Serializable
    data class CaptureSaved(
        val capture: Long,
        val entry: Capture,
        val work: WorkItem? = null,
        val request: AssistantRequest? = null,
    ) : Message

    /** What an `update-work`, `request-assistant` or `cancel-request` wrote, just before its `result`. */
    @Serializable
    data class WorkSaved(val work: Long, val entry: WorkItem, val request: AssistantRequest? = null) : Message

    /**
     * The Work item a `get-work` asked for, with its derived state, just before its `result`: the Captures
     * [WorkItem.captures] names, sorted or not, in that order, and its requests.
     */
    @Serializable
    data class WorkDetail(
        val work: Long,
        val entry: WorkItem,
        val captures: List<Capture> = emptyList(),
        val requests: List<AssistantRequest> = emptyList(),
    ) : Message

    /** The bridge is exiting. */
    @Serializable
    data class Bye(val reason: String) : Message

    /** A type this client does not know. The protocol says to ignore it. */
    data class Unknown(val type: String) : Message
}

@Serializable
data class ErrorBody(val code: ErrorCode, val message: String)

/** An error code; one added by a newer bridge keeps its name. */
@Serializable
@JvmInline
value class ErrorCode(val name: String) {
    companion object {
        val BAD_REQUEST = ErrorCode("bad-request")
        val INVALID_PANE = ErrorCode("invalid-pane")
        val NO_PANE = ErrorCode("no-pane")
        val UNSUPPORTED_KEY = ErrorCode("unsupported-key")
        val TOO_LARGE = ErrorCode("too-large")
        val TMUX = ErrorCode("tmux")
        val INTERNAL = ErrorCode("internal")
        /** No agent adapter knows the program in the pane. */
        val UNSUPPORTED = ErrorCode("unsupported")
        /** The task launcher failed; the message says why. */
        val TASK_FAILED = ErrorCode("task-failed")
        /** The task record changed since the client read it; list it again. */
        val CONFLICT = ErrorCode("conflict")
        /** Removing the worktree could lose work; the message says why. Nothing changed (ADR 0043). */
        val WOULD_LOSE_WORK = ErrorCode("would-lose-work")
    }
}
