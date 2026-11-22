package tech.xdge.muxa.v2

import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonArray
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.encodeToJsonElement
import kotlinx.serialization.json.put

/** A request on channel 0. The client adds `id`. */
sealed interface V2Request {
    data object Terminals : V2Request
    data class Attach(val terminal: String, val cols: Int, val rows: Int, val history: Int = 0) : V2Request
    data class Detach(val channel: Int) : V2Request
    /** No places: one folder (ADR 0068). */
    data class StartTask(
        val workspace: String,
        val title: String,
        val goal: String? = null,
        val places: List<PlaceSpec> = emptyList(),
        /** Deprecated and ignored: every agent gets all the Workspace's connections and MCP servers (ADR 0075). */
        val connections: List<String> = emptyList(),
        /** Deprecated and ignored (ADR 0075). */
        val mcp: List<String> = emptyList(),
        val plan: Boolean = false,
        val loop: LoopSpec? = null,
    ) : V2Request
    data object Watch : V2Request
    data object GetInbox : V2Request

    /** [today] is the device's local date, `YYYY-MM-DD`. */
    data class ParseCapture(val text: String, val today: String) : V2Request
    data class InboxAdd(val capture: Capture, val note: String? = null) : V2Request
    data class InboxMove(val item: String, val index: Int) : V2Request

    /** [until] is Unix seconds. */
    data class InboxLater(val item: String, val until: Long) : V2Request
    data class InboxDone(val item: String) : V2Request
    data class InboxReopen(val item: String) : V2Request
    data class InboxDelete(val item: String) : V2Request
    data class TaskReview(val terminal: String, val note: String? = null) : V2Request

    /** Closes the Task of a Review item. Refused with `would-lose-work` unless [worktree] is [WorktreeChoice.FORCE]. */
    data class InboxApprove(val item: String, val worktree: WorktreeChoice? = null) : V2Request

    /** Types [comment] to the Task's agent and resolves the Review item. */
    data class InboxRequestChanges(val item: String, val comment: String) : V2Request

    /** Runs a Failed item's Routine or Task again. */
    data class InboxRetry(val item: String) : V2Request
    data object Workspaces : V2Request
    data object Routines : V2Request
    data class RunRoutine(val workspace: String, val name: String) : V2Request
    data class ReaderListOf(val terminal: String, val kind: ReaderKind, val html: Boolean = false) : V2Request
    data class ReaderOpen(
        val terminal: String,
        val kind: ReaderKind,
        val path: String,
        val html: Boolean = false,
        val scope: DiffScope = DiffScope.UNCOMMITTED,
    ) : V2Request
    data class ReaderClose(val reader: Int) : V2Request
    data object Loops : V2Request
    data class LoopStart(val terminal: String, val spec: LoopSpec = LoopSpec(), val reset: Boolean = false) : V2Request
    data class LoopStop(val terminal: String) : V2Request
    data class ChatOpen(val terminal: String) : V2Request
    data class ChatClose(val chat: Int) : V2Request
    data object GetSettings : V2Request

    /** Applies [ops] in order, all or none; refused with `conflict` unless [revision] is the current one. */
    data class SettingsUpdate(val revision: String, val ops: List<SettingsOp>) : V2Request

    fun json(id: Long): JsonObject = buildJsonObject {
        put("id", id)
        fun type(t: String) = put("type", t)
        fun opt(k: String, v: String?) { if (v != null) put(k, v) }
        when (val r = this@V2Request) {
            Terminals -> type("terminals")
            is Attach -> { type("attach"); put("terminal", r.terminal); put("cols", r.cols); put("rows", r.rows); put("history", r.history) }
            is Detach -> { type("detach"); put("channel", r.channel) }
            is StartTask -> {
                type("start-task"); put("workspace", r.workspace); put("title", r.title); opt("goal", r.goal)
                put("places", buildJsonArray { r.places.forEach { add(placeJson(it)) } })
                put("connections", buildJsonArray { r.connections.forEach { add(JsonPrimitive(it)) } })
                put("mcp", buildJsonArray { r.mcp.forEach { add(JsonPrimitive(it)) } }); put("plan", r.plan)
                if (r.loop != null) put("loop", json.encodeToJsonElement(r.loop))
            }
            Watch -> type("watch")
            GetInbox -> type("inbox")
            is ParseCapture -> { type("parse-capture"); put("text", r.text); put("today", r.today) }
            is InboxAdd -> {
                type("inbox-add"); put("kind", JsonPrimitive(r.capture.kind.wire)); put("title", r.capture.title)
                opt("workspace", r.capture.workspace); opt("due", r.capture.due); opt("note", r.note)
            }
            is InboxMove -> { type("inbox-move"); put("item", r.item); put("index", r.index) }
            is InboxLater -> { type("inbox-later"); put("item", r.item); put("until", r.until) }
            is InboxDone -> { type("inbox-done"); put("item", r.item) }
            is InboxReopen -> { type("inbox-reopen"); put("item", r.item) }
            is InboxDelete -> { type("inbox-delete"); put("item", r.item) }
            is TaskReview -> { type("task-review"); put("terminal", r.terminal); opt("note", r.note) }
            is InboxApprove -> { type("inbox-approve"); put("item", r.item); opt("worktree", r.worktree?.wire) }
            is InboxRequestChanges -> { type("inbox-request-changes"); put("item", r.item); put("comment", r.comment) }
            is InboxRetry -> { type("inbox-retry"); put("item", r.item) }
            Workspaces -> type("workspaces")
            Routines -> type("routines")
            is RunRoutine -> { type("run-routine"); put("workspace", r.workspace); put("name", r.name) }
            is ReaderListOf -> { type("reader-list"); put("terminal", r.terminal); put("kind", r.kind.wire); put("html", r.html) }
            is ReaderOpen -> {
                type("reader-open"); put("terminal", r.terminal); put("kind", r.kind.wire); put("path", r.path)
                put("html", r.html); put("scope", r.scope.wire)
            }
            is ReaderClose -> { type("reader-close"); put("reader", r.reader) }
            Loops -> type("loops")
            is LoopStart -> {
                type("loop-start"); put("terminal", r.terminal); put("loop", json.encodeToJsonElement(r.spec)); put("reset", r.reset)
            }
            is LoopStop -> { type("loop-stop"); put("terminal", r.terminal) }
            is ChatOpen -> { type("chat-open"); put("terminal", r.terminal) }
            is ChatClose -> { type("chat-close"); put("chat", r.chat) }
            GetSettings -> type("settings")
            is SettingsUpdate -> {
                type("settings-update"); put("revision", r.revision)
                put("ops", buildJsonArray { r.ops.forEach { add(it.json()) } })
            }
        }
    }
}

private fun placeJson(p: PlaceSpec): JsonObject = buildJsonObject {
    when (p) {
        PlaceSpec.Folder -> put("kind", "folder")
        is PlaceSpec.Branch -> {
            put("kind", "branch"); put("repo", p.repo); put("branch", p.branch)
            if (p.base != null) put("base", p.base)
        }
        is PlaceSpec.Read -> { put("kind", "read"); put("repo", p.repo) }
    }
}
