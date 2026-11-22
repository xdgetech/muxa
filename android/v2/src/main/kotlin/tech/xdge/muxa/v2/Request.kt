package tech.xdge.muxa.v2

import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonArray
import kotlinx.serialization.json.buildJsonObject
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
        /** The Workspace's connections, besides the default ones (ADR 0069). */
        val connections: List<String> = emptyList(),
        val plan: Boolean = false,
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
                put("connections", buildJsonArray { r.connections.forEach { add(JsonPrimitive(it)) } }); put("plan", r.plan)
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
            Routines -> type("routines")
            is RunRoutine -> { type("run-routine"); put("workspace", r.workspace); put("name", r.name) }
            is ReaderListOf -> { type("reader-list"); put("terminal", r.terminal); put("kind", r.kind.wire); put("html", r.html) }
            is ReaderOpen -> {
                type("reader-open"); put("terminal", r.terminal); put("kind", r.kind.wire); put("path", r.path)
                put("html", r.html); put("scope", r.scope.wire)
            }
            is ReaderClose -> { type("reader-close"); put("reader", r.reader) }
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
