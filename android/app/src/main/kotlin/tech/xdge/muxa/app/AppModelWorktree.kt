package tech.xdge.muxa.app

import tech.xdge.muxa.protocol.ErrorCode
import tech.xdge.muxa.protocol.InboxAction
import tech.xdge.muxa.protocol.InboxItem
import tech.xdge.muxa.protocol.TaskRecord
import tech.xdge.muxa.protocol.TaskStatus
import tech.xdge.muxa.session.PaneSession

// Done and Drop remove the Task's worktree and branch (ADR 0043). When that could lose work the
// host changes nothing and says why; the user removes them anyway, or keeps the Task as it is.

/** An end of a Task that waits for the user, with the host's [reason]. Remove resends it with `force`. */
sealed interface LosingWork {
    val reason: String

    data class Status(val record: TaskRecord, val status: TaskStatus, override val reason: String) : LosingWork

    data class Answer(
        val item: InboxItem, val action: InboxAction, val text: String?, val option: Int?, override val reason: String,
    ) : LosingWork
}

/** The host's reason when it refused an end because removing the worktree could lose work. */
val PaneSession.Failure.lostWork: String?
    get() = (this as? PaneSession.Failure.Bridge)?.body?.takeIf { it.code == ErrorCode.WOULD_LOSE_WORK }?.message

/** Remove: the same end again, with `force`. */
fun AppModel.loseWork() {
    val pending = losingWork ?: return
    losingWork = null
    when (pending) {
        is LosingWork.Status -> setStatus(pending.record, pending.status, forceWorktree = true)
        is LosingWork.Answer -> sendAnswer(pending.item, pending.action, pending.text, pending.option, forceWorktree = true)
    }
}
