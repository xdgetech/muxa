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
