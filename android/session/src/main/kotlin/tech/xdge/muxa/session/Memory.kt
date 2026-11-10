package tech.xdge.muxa.session

import tech.xdge.muxa.protocol.InboxAction
import tech.xdge.muxa.protocol.InboxItem
import tech.xdge.muxa.protocol.ItemKind
import tech.xdge.muxa.protocol.ItemState
import tech.xdge.muxa.protocol.MemoryMode
import tech.xdge.muxa.protocol.WorkspaceInfo

// Workspace memory as the clients show it (ADR 0053 §7). The host checks the hash and appends
// to WORKSPACE.md; this only picks what the item screen offers and the words around it.

/**
 * [item] as the item screen offers it. A `memory` item offers Approve only where this client shows
 * its text whole: at `work` level 14 ([canKeepMemory]) with its `memory`. Other items keep the host's answers.
 */
fun InboxItem.memoryOffered(canKeepMemory: Boolean): InboxItem =
    if (kind != ItemKind.MEMORY || (canKeepMemory && memory != null)) this else copy(answers = answers - InboxAction.APPROVE)

/** The `memory` item of Task [task], as last listed: a Task has at most one. */
fun List<InboxItem>.memoryItemOf(task: String): InboxItem? = firstOrNull { it.kind == ItemKind.MEMORY && it.task == task }

/** What the Task detail says after a Task's end about its proposal to the workspace memory. */
enum class MemoryState {
    /** Open in the Inbox. */
    WAITING,

    /** Open, moved down by Later. */
    MOVED_DOWN,
    ADDED,
    REJECTED,

    /** Resolved by the host, or answered otherwise. */
    CLOSED,
}

val InboxItem.memoryState: MemoryState
    get() = when {
        state == ItemState.OPEN && laterAt != null -> MemoryState.MOVED_DOWN
        state == ItemState.OPEN -> MemoryState.WAITING
        state == ItemState.ANSWERED && answer == InboxAction.APPROVE -> MemoryState.ADDED
        state == ItemState.ANSWERED && answer == InboxAction.REJECT -> MemoryState.REJECTED
        else -> MemoryState.CLOSED
    }

/** The Task detail's line: "Added to the workspace memory", else the proposal with its item's state. */
val MemoryState.line: String
    get() = when (this) {
        MemoryState.ADDED -> "Added to the workspace memory"
        MemoryState.WAITING -> "$PROPOSED · Waits for your approval in the Inbox"
        MemoryState.MOVED_DOWN -> "$PROPOSED · Moved down in the Inbox"
        MemoryState.REJECTED -> "$PROPOSED · Rejected"
        MemoryState.CLOSED -> "$PROPOSED · Closed"
    }

private const val PROPOSED = "Proposed to the workspace memory"

/** What a workspace's `memory` mode means for the user. An unknown value reads as unknown, as PROTOCOL.md says. */
val MemoryMode.label: String
    get() = when (this) {
        MemoryMode.REVIEW -> "A Task's end asks you in the Inbox before it adds to the workspace memory."
        MemoryMode.AUTO -> "A Task's end adds to the workspace memory at once."
        MemoryMode.OFF -> "Workspace memory is off."
        else -> "Workspace memory: unknown"
    }

/** [workspace] when the host reports its memory mode (`work:14`); null from older hosts or for an unknown name. */
fun List<WorkspaceInfo>.memoryOf(workspace: String?): WorkspaceInfo? = firstOrNull { it.name == workspace }?.takeIf { it.memory != null }
