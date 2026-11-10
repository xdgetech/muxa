package tech.xdge.muxa.session

import tech.xdge.muxa.protocol.AssistResult
import tech.xdge.muxa.protocol.AssistantInfo
import tech.xdge.muxa.protocol.AssistantRequest
import tech.xdge.muxa.protocol.DelegateInfo
import tech.xdge.muxa.protocol.InboxAction
import tech.xdge.muxa.protocol.InboxItem
import tech.xdge.muxa.protocol.ItemKind
import tech.xdge.muxa.protocol.ItemState
import tech.xdge.muxa.protocol.ResultWrite
import tech.xdge.muxa.protocol.WriteProposal
import tech.xdge.muxa.protocol.Writes

// Write proposals as the clients show them (ADR 0052 §7). The host checks the hash, makes the
// writer's Task and never claims the write happened; this only picks the rows and their words.

/**
 * [item] as the item screen offers it. A `write` item offers Approve only where this client shows
 * its content whole: at `work` level 13 ([canWrite]) with its `write`. Other items keep the host's answers.
 */
fun InboxItem.offered(canWrite: Boolean): InboxItem =
    if (kind != ItemKind.WRITE || (canWrite && write != null)) this else copy(answers = answers - InboxAction.APPROVE)

/** The write came from an older version of its request: its item stays open and says so. */
fun WriteProposal.isOlder(requests: List<AssistantRequest>): Boolean =
    requests.firstOrNull { it.id == request }?.let { version < it.version } == true

/** A Result older than its request's current version: its writes are "from an older result". */
fun AssistResult.isOlder(request: AssistantRequest): Boolean = version < request.version

/** Who proposed the write: the staff member, else the workspace's assistant. */
fun WriteProposal.proposer(workspace: String): String = worker ?: delegateName(workspace)

/** What Work detail says about one of a Result's writes. */
enum class WriteState {
    /** No item yet: the host makes it on its next tick. */
    PROPOSED,

    /** Its item is open in the Inbox. */
    WAITING,
    REJECTED,

    /** Answered otherwise, or resolved by the host, with no Task. */
    CLOSED,

    /** Approved: the writer's Task exists. Its own state says the rest. */
    STARTED,
}

/** The writer's Task: on the Result once the host recorded it, else on its item. */
fun ResultWrite.taskOf(items: List<InboxItem>): String? = task ?: itemOf(items)?.task

/** The write's Inbox item, as last listed. */
fun ResultWrite.itemOf(items: List<InboxItem>): InboxItem? = item?.let { id -> items.firstOrNull { it.id == id } }

fun ResultWrite.state(items: List<InboxItem>): WriteState {
    if (taskOf(items) != null) return WriteState.STARTED
    val i = itemOf(items) ?: return WriteState.PROPOSED
    return when {
        i.state == ItemState.OPEN -> WriteState.WAITING
        i.state == ItemState.ANSWERED && i.answer == InboxAction.REJECT -> WriteState.REJECTED
        else -> WriteState.CLOSED
    }
}

val WriteState.label: String
    get() = when (this) {
        WriteState.PROPOSED -> "Proposed"
        WriteState.WAITING -> "Waits for your approval in the Inbox"
        WriteState.REJECTED -> "Rejected"
        WriteState.CLOSED -> "Closed without a Task"
        WriteState.STARTED -> "Task started"
    }

/** [workspace]'s delegate when it has a writer or says whether it takes writes; null from a host before level 13. */
fun AssistantInfo?.writesOf(workspace: String?): DelegateInfo? =
    this?.delegates?.firstOrNull { it.workspace == workspace }?.takeIf { it.writer != null || it.writes != null }

/** What a workspace's `writes` mode means for the user. An unknown value reads as unknown, as PROTOCOL.md says. */
val Writes.label: String
    get() = when (this) {
        Writes.REVIEW -> "Each write waits for your approval in the Inbox."
        Writes.NONE -> "Takes no writes. Changes go into Next."
        else -> "Writes: unknown"
    }
