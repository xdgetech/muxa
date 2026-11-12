package tech.xdge.muxa.session

import tech.xdge.muxa.protocol.ChatPrompt
import tech.xdge.muxa.protocol.InboxAction
import tech.xdge.muxa.protocol.InboxItem
import tech.xdge.muxa.protocol.ItemKind
import tech.xdge.muxa.protocol.TaskProposal

// The assistant proposes Tasks (ADR 0060 §8). The wire has the answers; this picks which one is
// Start Task, and the words a plan item puts on approve, reject and reply.

/** The Task the item offers to start: a `result` item's `result.task`, or a `new-task` item's proposal. */
val InboxItem.proposedTask: TaskProposal?
    get() = when (kind) {
        ItemKind.RESULT -> result?.task
        ItemKind.NEW_TASK -> newTask
        else -> null
    }

/**
 * The answer that starts the proposed Task (ADR 0060 §2, §3): `start-task` beside a hand-off, else
 * `approve`. Null when the host offers neither, or the item proposes no Task.
 */
val InboxItem.startTaskAction: InboxAction?
    get() {
        if (proposedTask == null) return null
        if (InboxAction.START_TASK in answers) return InboxAction.START_TASK
        val approveStarts = kind == ItemKind.NEW_TASK || result?.handoff == null
        return InboxAction.APPROVE.takeIf { approveStarts && it in answers }
    }

/** Claude Code's plan menu waits in the Task (ADR 0060 §5): approve leaves plan mode, asking before each edit. */
val InboxItem.isPlan: Boolean get() = kind == ItemKind.ASK && prompt?.kind == ChatPrompt.Kind.PLAN

/** An item that started a Task from a proposal or a write: Open its Task (ADR 0060 §6). */
val InboxItem.startedTask: String?
    get() = task?.takeIf { kind == ItemKind.RESULT || kind == ItemKind.NEW_TASK || kind == ItemKind.WRITE }
