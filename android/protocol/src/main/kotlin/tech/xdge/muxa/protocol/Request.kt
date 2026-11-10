package tech.xdge.muxa.protocol

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

/** Sent by the client. Every request has an [id], echoed in its `result`. */
sealed interface Request {
    val id: Long

    @Serializable data class Ping(override val id: Long) : Request
    @Serializable data class List(override val id: Long) : Request

    /** Show this pane. `screen` and `output` for it carry this request's id as `watch`. */
    @Serializable data class Watch(override val id: Long, val pane: String) : Request
    @Serializable data class Unwatch(override val id: Long) : Request

    /** The pane's last [lines] scrollback lines and its screen, once (capability `history`). */
    @Serializable data class History(override val id: Long, val pane: String, val lines: Int? = null) : Request

    /** The grid the pane area shows at the default font (capability `viewport`, ADR 0020). The bridge sizes the watched pane's window for it. */
    @Serializable data class Viewport(override val id: Long, val cols: Int, val rows: Int) : Request

    /** Named keys, e.g. `C-c`, `Up`, `S-Enter`. */
    @Serializable data class Keys(override val id: Long, val pane: String, val keys: kotlin.collections.List<String>) : Request

    /** Typed literally. */
    @Serializable data class Text(override val id: Long, val pane: String, val text: String) : Request

    /** Raw key bytes, for a client that encodes keys itself. */
    @Serializable data class RawBytes(override val id: Long, val pane: String, val data: Bytes) : Request

    /** Pasted, bracketed when the program asked for it. */
    @Serializable data class Paste(override val id: Long, val pane: String, val text: String) : Request

    /** Follow the agent conversation in this pane (capability `chat`). */
    @Serializable data class Chat(override val id: Long, val pane: String) : Request
    @Serializable data class Unchat(override val id: Long) : Request

    /** This pane's Markdown documents (capability `docs`); a `doc-list` comes first. */
    @Serializable data class Docs(override val id: Long, val pane: String) : Request

    /** Open a listed document and follow its changes. */
    @Serializable data class Doc(override val id: Long, val pane: String, val path: String) : Request
    @Serializable data class Undoc(override val id: Long) : Request

    /** The files that differ from HEAD (capability `changes`); a `change-list` comes first. */
    @Serializable data class Changes(override val id: Long, val pane: String) : Request

    /** Show and follow one listed file's diff. [scope] null means the uncommitted list. */
    @Serializable
    data class Diff(override val id: Long, val pane: String, val path: String, val scope: DiffScope? = null) : Request
    @Serializable data class Undiff(override val id: Long) : Request

    /** The image files in the pane's repo (capability `images`); an `image-list` comes first. */
    @Serializable data class Images(override val id: Long, val pane: String) : Request

    /** Open a listed image and follow its changes. */
    @Serializable data class Image(override val id: Long, val pane: String, val path: String) : Request
    @Serializable data class Unimage(override val id: Long) : Request

    /** Start a task on [branch] of the repo at [repo] (capability `tasks`). */
    @Serializable
    data class NewTask(override val id: Long, val repo: String, val branch: String, val base: String? = null) : Request

    /** Close a task's session (capability `close-task`). The branch always stays. */
    @Serializable data class CloseTask(override val id: Long, val session: String, val mode: CloseMode) : Request

    /** The task records and the workspaces (capability `task-records`); a `task-list` comes first. */
    @Serializable data class ListTasks(override val id: Long) : Request

    /** Record a new task and start an agent for it; a `task-saved` comes first. Without [repo] it gets a directory. */
    @Serializable
    data class CreateTask(
        override val id: Long,
        val workspace: String,
        val title: String,
        val goal: String,
        val project: String? = null,
        val repo: String? = null,
        val branch: String? = null,
        val base: String? = null,
        /** Also start a Task Loop (capability `task-loop`, ADR 0019). */
        val loop: LoopSpec? = null,
        val worker: String? = null,
        /** The Work item the Task is for (`work` level 9, ADR 0048). The same [work] and [workRev] return the Task already made. */
        val work: String? = null,
        /** With [work]: the item's `rev` the client saw. */
        val workRev: Long? = null,
    ) : Request

    /**
     * Set a task record's [status]. [rev] is the one the client saw; a stale one fails with `conflict`.
     * [worktree] goes with `done` and `dropped`; without it the worktree stays.
     */
    @Serializable
    data class UpdateTask(
        override val id: Long, val task: String, val rev: Long, val status: TaskStatus, val worktree: WorktreeEnd? = null,
    ) : Request

    /**
     * Restart a task's agent in the same pane, after a handoff (capability `task-handoff`, ADR 0018).
     * Refused while the agent runs, unless [force]. A `task-saved` comes first.
     */
    @Serializable
    data class RestartAgent(
        override val id: Long, val task: String, val force: Boolean = false,
        /** A worker name or `default`; null keeps the current worker. [rev] is checked only when sent. */
        val worker: String? = null, val rev: Long? = null,
    ) : Request

    /** Start a task's Task Loop again (capability `task-loop`, ADR 0019); [reset] counts attempts from zero. A `task-saved` comes first. */
    @Serializable
    data class RecoverRestart(override val id: Long, val task: String, val rev: Long, val operation: String, val action: RestartRecoveryAction) : Request

    @Serializable
    data class StartLoop(override val id: Long, val task: String, val loop: LoopSpec, val reset: Boolean = false) : Request

    /** Stop a task's Task Loop (capability `task-loop`). A `task-saved` comes first. */
    @Serializable data class StopLoop(override val id: Long, val task: String) : Request

    /** The host's Routines (capability `work` level 1, ADR 0026); a `routine-list` comes first. */
    @Serializable data class ListRoutines(override val id: Long) : Request

    /** Start a run of an enabled Routine now. Needs the host daemon. A `routine-saved` comes first. */
    @Serializable data class RunRoutine(override val id: Long, val workspace: String, val routine: String) : Request

    /** The stored Inbox entries (capability `work` level 1); an `inbox-list` comes first. */
    @Serializable data class ListInbox(override val id: Long) : Request

    /**
     * Answer a stored entry or an Inbox item (its `i-` id). [until] goes with `later`, [text] with `redo`,
     * `change-course` and `reply`, [option] with `choose`. An `inbox-saved` comes first.
     */
    @Serializable
    data class UpdateInbox(
        override val id: Long,
        val entry: String,
        val action: InboxAction,
        val until: Long? = null,
        val text: String? = null,
        val option: Int? = null,
        /** As `update-task`'s, for an answer that makes the Task done or dropped. */
        val worktree: WorktreeEnd? = null,
        /**
         * With `approve` on a `write` item (`work` level 13, ADR 0052): the hash of the content the client showed
         * whole. The host refuses an approve on a write without it.
         */
        val hash: String? = null,
    ) : Request

    /**
     * Remove every Inbox item made more than [olderThanDays] days ago, except protected ones
     * (capability `work` level 8, ADR 0046). An `inbox-deleted` comes first.
     */
    @Serializable data class DeleteInbox(override val id: Long, val olderThanDays: Int) : Request

    /**
     * An agent session in a repo's main checkout, to ask about its code: no Task, no
     * worktree (capability `work` level 2, ADR 0031). [repo] is as `create-task`'s.
     * A `session-started` comes first.
     */
    @Serializable data class StartSession(override val id: Long, val workspace: String, val repo: String) : Request

    /**
     * Save the user's input (`work` level 9, ADR 0048). The same [clientId] returns the stored Capture and
     * writes nothing. [assist] also makes a Work item and asks the assistant; it needs [workspace].
     * A `capture-saved` comes first.
     */
    @Serializable
    data class Capture(
        override val id: Long,
        val clientId: String,
        val text: String,
        val source: CaptureSource? = null,
        val workspace: String? = null,
        val assist: AssistSpec? = null,
    ) : Request

    /**
     * Sort an unsorted Capture. [work] goes with `link`; [title] with `work`, where null takes the text's
     * first line. A stale [rev] is `conflict`. A `capture-saved` comes first.
     */
    @Serializable
    data class SortCapture(
        override val id: Long,
        val capture: String,
        val rev: Long,
        val action: SortAction,
        val work: String? = null,
        val title: String? = null,
        val workspace: String? = null,
    ) : Request

    /**
     * Change a Work item. [title], [workspace] and [due] (level 10) go with `edit`, [into] with `merge`.
     * A stale [rev] is `conflict`. A `work-saved` comes first.
     */
    @Serializable
    data class UpdateWork(
        override val id: Long,
        val work: String,
        val rev: Long,
        val action: WorkAction,
        val title: String? = null,
        val workspace: String? = null,
        val into: String? = null,
        val due: Due? = null,
    ) : Request

    /**
     * Ask a workspace's agent to work on a Work item; needs the host daemon. [workspace] null takes the item's.
     * The same [work] and [rev] return the request already made. A `work-saved` comes first. [worker] (level 12)
     * names a staff member of the workspace; null asks its lead.
     */
    @Serializable
    data class RequestAssistant(
        override val id: Long,
        val work: String,
        val rev: Long,
        val scope: AssistScope,
        val workspace: String? = null,
        val purpose: String? = null,
        val worker: String? = null,
    ) : Request

    /** Stop a request's [version]. A `work-saved` comes first. */
    @Serializable data class CancelRequest(override val id: Long, val request: String, val version: Long) : Request

    /** One Work item with every Capture it links, sorted or not, and its requests. A `work-detail` comes first. */
    @Serializable data class GetWork(override val id: Long, val work: String) : Request

    /**
     * Write a result's or a plan's proposed notes to the live `NOTES.md` (`work` level 10, ADR 0049). [source]
     * is a request `q-…` or a plan `p-…`; [version] the request's version or the plan's `rev`; [base] the
     * proposal's. A changed live file is `conflict`. A `work-saved` or `plan-saved` comes first. [step] (level 12)
     * is the step of a request's version whose result proposed the notes; null is the version's last step.
     */
    @Serializable
    data class ApplyNotes(
        override val id: Long,
        val source: String,
        val version: Long,
        val base: String,
        val step: Int? = null,
    ) : Request

    /** Start an overall run now (`work` level 10); [minutes] is the time the user has. A `plan-saved` comes first. */
    @Serializable data class PlanNow(override val id: Long, val minutes: Int? = null) : Request

    /**
     * Keep a policy in the user's words (`work` level 11, ADR 0050). [scope] is `all` or a workspace; [from]
     * and [until] are epoch seconds. A `policy-saved` comes first.
     */
    @Serializable
    data class AddPolicy(
        override val id: Long,
        val text: String,
        val scope: String,
        val from: Long? = null,
        val until: Long? = null,
    ) : Request

    /** Edit or delete a policy. A stale [rev] is `conflict`. `edit` sends a `policy-saved` first; `delete` only the `result`. */
    @Serializable
    data class UpdatePolicy(
        override val id: Long,
        val policy: String,
        val rev: Long,
        val action: PolicyAction,
        val text: String? = null,
        val scope: String? = null,
        val from: Long? = null,
        val until: Long? = null,
    ) : Request

    /**
     * Reorder Today's rows ([order]), or remove one ([row], with the reason in [text]), on the newest `resulted`
     * plan. A `plan-saved` comes first.
     */
    @Serializable
    data class UpdatePlan(
        override val id: Long,
        val plan: String,
        val rev: Long,
        val action: PlanAction,
        val order: kotlin.collections.List<String> = emptyList(),
        val row: String? = null,
        val text: String? = null,
    ) : Request

    /** Remove a correction `x-…` or a hypothesis `h-…`; [record] because [id] is the request's. */
    @Serializable data class Forget(override val id: Long, val record: String) : Request

    /** The client is leaving; the bridge exits. */
    @Serializable data class Bye(override val id: Long) : Request
}

/** What happens to a task's worktree when it closes. */
@Serializable
enum class CloseMode {
    /** Remove it when the branch is merged; otherwise refuse. */
    @SerialName("close") CLOSE,
    @SerialName("keep") KEEP,
    /** Remove it even when the branch is not merged. */
    @SerialName("force") FORCE,
}

/** What a Task's end does with its worktree and branch (`work` level 7, ADR 0043). */
@Serializable
enum class WorktreeEnd {
    /** Removes them, unless that could lose work: then `would-lose-work` and nothing changes. */
    @SerialName("remove") REMOVE,
    @SerialName("force") FORCE,
}
