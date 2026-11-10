package tech.xdge.muxa.protocol

import kotlinx.serialization.Serializable

// External writes only when asked (ADR 0052), at `work` level 13. A Result proposes writes; the
// user approves each exact text in the Inbox; one writer worker performs it as a Task.

/** A write a Result proposes, as the host stored it. Never logged. */
@Serializable
data class ResultWrite(
    /** 1 to 200 characters on one line, e.g. "Post to #release". */
    val action: String,
    /** 1 to 500 bytes on one line: `slack:#release`, `linear:ENG-12`, a URL. */
    val target: String,
    /** The exact text to write, 1 byte to 8 KiB. */
    val content: String,
    /** `sha256:` and the hex SHA-256 of [action], NUL, [target], NUL, [content]. */
    val hash: String,
    /** Why the member proposes it. */
    val reason: String? = null,
    /** The `write` Inbox item that asks the user, once made. */
    val item: String? = null,
    /** The writer's Task, once approved. */
    val task: String? = null,
)

/** A `write` Inbox item's payload: the exact write approve hands the writer. */
@Serializable
data class WriteProposal(
    /** `q-…`. */
    val request: String,
    /** The request version whose Result proposed it; may be older than the current one. */
    val version: Long,
    /** The step of [version] whose Result proposed it. Null: step 1. */
    val step: Int? = null,
    /** The staff member whose Result proposed it. Null: the workspace's agent. */
    val worker: String? = null,
    val action: String,
    val target: String,
    /** Shown whole, as plain text, before Approve. */
    val content: String,
    val hash: String,
)

/** The worker that performs a workspace's approved writes, in `inbox-list`. */
@Serializable
data class WriterInfo(
    /** A host `[[worker]]` name. */
    val worker: String,
    /** The worker's job description, at most 1 KiB. */
    val description: String,
)

/** Whether a workspace takes write proposals. Unknown values stay as their name: show them as unknown. */
@Serializable
@JvmInline
value class Writes(val raw: String) {
    companion object {
        /** A Result with `writes` fails the check; changes go into `next`. */
        val NONE = Writes("none")

        /** The user approves each write; the writer performs it. */
        val REVIEW = Writes("review")
    }
}
