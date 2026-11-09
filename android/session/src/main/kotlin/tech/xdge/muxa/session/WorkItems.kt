package tech.xdge.muxa.session

import tech.xdge.muxa.protocol.Capture
import tech.xdge.muxa.protocol.WorkItem
import tech.xdge.muxa.protocol.WorkState
import tech.xdge.muxa.protocol.WorkspaceInfo
import java.util.UUID

// Captures and Work items as the Inbox shows them (ADR 0046 §10). The host orders and derives
// them; this only picks what to show and in which words.

/** Done or withdrawn: only the user's `reopen` brings it back. */
val WorkItem.isClosed: Boolean get() = state == WorkState.DONE || state == WorkState.WITHDRAWN

/** The Inbox's Work section: the host's order (open, then moved down by Later), without closed items. */
fun List<WorkItem>.listed(): List<WorkItem> = filterNot { it.isClosed }

/** What a row says about the state. An unknown state reads as open, as PROTOCOL.md says. */
val WorkItem.stateLabel: String
    get() = when (state.shown) {
        WorkState.IN_PROGRESS -> "in progress"
        WorkState.WAITING_DECISION -> "waiting on decision"
        WorkState.DONE -> "done"
        WorkState.WITHDRAWN -> "withdrawn"
        else -> "open"
    }

/** Later moved it down (ADR 0040). It stays listed. */
val WorkItem.isLater: Boolean get() = laterAt != null && !isClosed

/** The first line with text, trimmed and cut to [max] characters; a Capture can be long. */
fun firstLine(text: String, max: Int = TITLE_MAX): String =
    text.lineSequence().map { it.trim() }.firstOrNull { it.isNotEmpty() }.orEmpty().take(max)

val Capture.firstLine: String get() = firstLine(text)

/** At most this many characters in a Work item's title, as the host keeps it. */
const val TITLE_MAX = 200

/** At most this many bytes in a Capture's text (PROTOCOL.md). */
const val CAPTURE_MAX_BYTES = 16 * 1024

/**
 * What the Capture sheet edits. [clientId] is made once per capture, so a second Save or a
 * resend after a lost connection makes no second Capture on the host.
 */
data class CaptureDraft(
    val text: String = "",
    /** Null: no workspace yet. */
    val workspace: String? = null,
    val clientId: String = UUID.randomUUID().toString(),
) {
    /** What to fix before Save; null when it can be sent. */
    val problem: String?
        get() = when {
            text.isBlank() -> "Write something to save."
            text.trim().toByteArray().size > CAPTURE_MAX_BYTES -> "This is too long. Keep it under 16 KiB."
            else -> null
        }

    /** "Start a Task instead…": the text is the goal, its first line the title. */
    fun taskDraft(workspaces: List<WorkspaceInfo>): TaskRecordDraft = TaskRecordDraft(
        workspace = workspace ?: workspaces.firstOrNull()?.name.orEmpty(),
        title = firstLine(text),
        goal = text.trim(),
    )
}

/** Start a Task for [item]: the New Task form, linked to the item at the `rev` the client saw. */
fun TaskRecordDraft.Companion.forWork(item: WorkItem, workspaces: List<WorkspaceInfo>): TaskRecordDraft = TaskRecordDraft(
    workspace = item.workspace?.takeIf { w -> workspaces.any { it.name == w } } ?: workspaces.firstOrNull()?.name.orEmpty(),
    title = item.title,
    goal = item.title,
    work = item.id,
    workRev = item.rev,
)
