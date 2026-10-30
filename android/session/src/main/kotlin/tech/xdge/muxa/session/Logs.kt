package tech.xdge.muxa.session

import tech.xdge.muxa.protocol.Agent
import tech.xdge.muxa.protocol.ChatEntry
import tech.xdge.muxa.protocol.ChatPrompt
import tech.xdge.muxa.protocol.DiffScope
import tech.xdge.muxa.protocol.Message

/** The conversation of the agent in one pane, as `chat-entries` build it up. */
data class ChatLog(
    val pane: String,
    val entries: List<ChatEntry> = emptyList(),
    /** The agent's state as of the last update; null when unknown. */
    val status: Agent? = null,
    /** What the agent waits on as of the last update; null when none is known. */
    val prompt: ChatPrompt? = null,
    /** The adapter, e.g. `claude-code`. */
    val agent: String? = null,
    /** Older history exists but was not sent. */
    val truncated: Boolean = false,
    /** Why the bridge stopped following, if it did. */
    val ended: String? = null,
    /** The first update has arrived. */
    val loaded: Boolean = false,
    private val seq: Long = 0,
) {
    /**
     * Applies one update. An update at or below the last `seq` was already covered,
     * for example by a resend, and is dropped.
     */
    fun apply(update: Message.ChatEntries): ChatLog {
        if (update.seq <= seq) return this
        val list = if (update.reset) mutableListOf() else entries.toMutableList()
        val index = list.withIndex().associate { it.value.id to it.index }.toMutableMap()
        for (entry in update.entries) {
            val i = index[entry.id]
            if (i != null) {
                list[i] = entry
            } else {
                index[entry.id] = list.size
                list += entry
            }
        }
        return copy(
            entries = list,
            status = update.status,
            prompt = update.prompt,
            agent = if (update.reset) update.agent else agent,
            truncated = if (update.reset) update.truncated else truncated,
            ended = null,
            loaded = true,
            seq = update.seq,
        )
    }
}

/** The open document, as `doc-content` updates it. */
data class DocLog(
    val pane: String,
    val path: String,
    /** Null until the first content arrives. */
    val text: String? = null,
    /** Epoch seconds of the file's last change. */
    val mtime: Long? = null,
    /** Why the bridge stopped following it, if it did. */
    val ended: String? = null,
)

/** The open diff, as `diff-content` updates it. */
data class DiffLog(
    val pane: String,
    val path: String,
    val scope: DiffScope = DiffScope.UNCOMMITTED,
    /** The unified diff; null until it arrives, and for a binary file. */
    val text: String? = null,
    val binary: Boolean = false,
    /** Why the bridge stopped following it, if it did. */
    val ended: String? = null,
) {
    /** Something arrived: text, or word that the file is binary. */
    val loaded: Boolean get() = text != null || binary
}

/** The open image, as `image-content` updates it. */
class ImageLog(
    val pane: String,
    val path: String,
    /** The file as it is on the host; null until it arrives. */
    val data: ByteArray? = null,
    val mtime: Long? = null,
    /** Why the bridge stopped following it, if it did. */
    val ended: String? = null,
) {
    fun copy(data: ByteArray? = this.data, mtime: Long? = this.mtime, ended: String? = this.ended) =
        ImageLog(pane, path, data, mtime, ended)

    override fun equals(other: Any?) = other is ImageLog && pane == other.pane && path == other.path &&
        mtime == other.mtime && ended == other.ended && data.contentEquals(other.data)

    override fun hashCode() = listOf(pane, path, mtime, ended, data?.contentHashCode()).hashCode()

    // Never the bytes: image content stays out of logs.
    override fun toString() = "ImageLog($pane, ${data?.size})"
}
