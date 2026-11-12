package tech.xdge.muxa.session

import tech.xdge.muxa.protocol.InboxAction
import tech.xdge.muxa.protocol.InboxItem
import tech.xdge.muxa.protocol.ItemKind
import tech.xdge.muxa.protocol.PlanRow
import tech.xdge.muxa.protocol.RunBudget

// Today rows say what to do (ADR 0059). The verbs, the primary action and who gets Send are the
// client's, from the target's kind; muxa for tmux (`tmux/scripts/inbox.sh`) uses the same words.

/**
 * What a Today row asks the user to do (ADR 0059 §1): "Start" for a Work item, a verb from the kind
 * of [item], and "Open" for an unknown kind or an item no longer listed.
 */
fun rowVerb(row: PlanRow, item: InboxItem?): String {
    if (row.work != null) return "Start"
    val kind = item?.kind ?: return "Open"
    if (kind == ItemKind.BRIEFING) return if (QUESTION_END.containsMatchIn(item.question)) "Decide" else "Read"
    return VERBS[kind] ?: "Open"
}

private val QUESTION_END = Regex("""\?\s*$""")

private val VERBS = mapOf(
    ItemKind.REVIEW to "Review changes",
    ItemKind.ASK to "Allow or deny",
    ItemKind.RESULT to "Read result",
    ItemKind.WRITE to "Check text to send",
    ItemKind.FAILURE to "Fix",
    ItemKind.MEMORY to "Check memory note",
    ItemKind.PROPOSAL to "Approve or reject",
    ItemKind.SORT to "Sort capture",
    ItemKind.DELEGATE to "Approve request",
    ItemKind.HYPOTHESIS to "Confirm or reject",
    ItemKind.HANDOFF to "Approve hand-off",
    ItemKind.NEW_TASK to "Start Task",
)

/** A `review` or `ask` whose Task has a session: the primary action opens that session (ADR 0059 §3). */
val InboxItem.opensSession: Boolean get() = (kind == ItemKind.REVIEW || kind == ItemKind.ASK) && session != null

/** The primary action's label (ADR 0059 §3): open the Task, else move to the target. Null [item]: a Work item. */
fun primaryLabel(item: InboxItem?): String = when {
    item == null -> "Open the Work item"
    item.opensSession -> "Open the Task (${if (item.kind == ItemKind.ASK) "allow or deny" else "review"})"
    else -> "Open the ${TARGET_WORDS[item.kind] ?: "item"}"
}

private val TARGET_WORDS = mapOf(
    ItemKind.RESULT to "result",
    ItemKind.WRITE to "text to send",
    ItemKind.BRIEFING to "briefing",
    ItemKind.FAILURE to "failure",
    ItemKind.NEW_TASK to "proposed Task",
)

/** What `s` sends on a Today row, and to whom (ADR 0059 §4). */
sealed class TodaySend {
    abstract val label: String

    /** `reply` on the item: the Task's agent, the member who wrote it, or the secretary. */
    data class Reply(val item: InboxItem, override val label: String) : TodaySend()

    /** `ask-agent` on the item: the workspace's Ask session. */
    data class AskAgent(val item: InboxItem, override val label: String) : TodaySend()

    /** `start-session` with `text` and `work` on the secretary's root Ask session (level 17). */
    data class Secretary(val work: String, val workspace: String, override val label: String) : TodaySend()
}

/** Who gets [this] item's text, or null when it takes none. Labels end in "…": a text follows. */
val InboxItem.todaySend: TodaySend?
    get() = when {
        InboxAction.REPLY in answers -> TodaySend.Reply(this, replyLabel)
        InboxAction.ASK_AGENT in answers ->
            TodaySend.AskAgent(this, "Send to the $workspace Ask session, with this ${if (kind == ItemKind.BRIEFING) "briefing" else "item"}…")
        else -> null
    }

private val InboxItem.replyLabel: String
    get() = when {
        kind == ItemKind.RESULT -> "Send to ${result?.worker ?: "the assistant"}…"
        kind == ItemKind.WRITE -> "Revise: send to ${write?.worker ?: "the assistant"}…"
        kind == ItemKind.NEW_TASK -> "Send to the secretary…"
        isPlan -> "Keep planning & send to the Task's agent…"
        kind == ItemKind.ASK -> "Deny & send to the Task's agent…"
        else -> "Send to the Task's agent…"
    }

/** A Work item's Send: the secretary, [secretary] being the overall workspace at level 17; null without one. */
fun workSend(work: String, secretary: String?): TodaySend.Secretary? =
    secretary?.let { TodaySend.Secretary(work, it, "Send to the secretary ($it), with this Work item…") }

/** "2 of 3 runs left today", for Re-rank (ADR 0059 §5). */
val RunBudget.label: String get() = "$left of $max runs left today"
