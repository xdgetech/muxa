package tech.xdge.muxa.protocol

import kotlinx.serialization.Serializable

// Workspace memory (ADR 0053), at `work` level 14. A Task's end proposes the text under "For the
// workspace" in its TASK.md; the user approves it in the Inbox before the host appends it to
// `<root>/.muxa/WORKSPACE.md`.

/** A `memory` Inbox item's payload: the exact text approve appends. Never logged. */
@Serializable
data class MemoryProposal(
    /** The Task whose close-out proposed it. */
    val task: String,
    /** The Task's title; the appended heading carries it. */
    val title: String,
    /** Shown whole, as plain text, before Approve. At most 4 KiB. */
    val text: String,
    /** `sha256:` and the hex SHA-256 of [task], NUL, [title], NUL, [text]. */
    val hash: String,
)

/** What a workspace does with a Task's "For the workspace" at its end. Unknown values stay as their name: show them as unknown. */
@Serializable
@JvmInline
value class MemoryMode(val raw: String) {
    companion object {
        /** The user approves each addition in the Inbox. */
        val REVIEW = MemoryMode("review")

        /** The host appends at once, within the cap; otherwise it asks as [REVIEW]. */
        val AUTO = MemoryMode("auto")

        /** Nothing is read and nothing is made. */
        val OFF = MemoryMode("off")
    }
}
