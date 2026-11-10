package tech.xdge.muxa.session

import tech.xdge.muxa.protocol.ChatPrompt
import tech.xdge.muxa.protocol.InboxAction
import tech.xdge.muxa.protocol.InboxItem
import tech.xdge.muxa.protocol.ItemKind
import tech.xdge.muxa.protocol.ItemState

// The Inbox as a list of questions (ADR 0033): which items wait, and which answers the item
// screen shows where. The host decides what an item accepts (`answers`); this only places it.

/**
 * Waits for the user. An item Later moved down (`laterAt`) is open and still shown (ADR 0040).
 * A host before ADR 0040 hid a `later` item until its time ([now], epoch seconds).
 */
fun InboxItem.isWaiting(now: Long): Boolean = when (state) {
    ItemState.OPEN -> true
    ItemState.LATER -> (until ?: 0) <= now
    else -> false
}

/** The items to show, in the host's order. */
fun List<InboxItem>.waiting(now: Long = System.currentTimeMillis() / 1000): List<InboxItem> = filter { it.isWaiting(now) }

val InboxItem.isUnread: Boolean get() = readAt == null

/** Delete Old keeps it (ADR 0046). */
val InboxItem.isProtected: Boolean get() = protectedAt != null

/** The seconds in one of Delete Old's days, as the host counts them. */
private const val DAY_SECONDS = 86_400L

/**
 * The items `delete-inbox` with [olderThanDays] removes, open or closed: made before the host's
 * cutoff and not protected. The confirmation names their count (docs/inbox.md).
 */
fun List<InboxItem>.deletable(olderThanDays: Int, now: Long = System.currentTimeMillis() / 1000): List<InboxItem> {
    val cutoff = now - olderThanDays * DAY_SECONDS
    return filter { !it.isProtected && it.createdAt < cutoff }
}

/** An agent is blocked on it, or it stopped: what a phone opens on after connecting. */
val InboxItem.isUrgent: Boolean get() = kind == ItemKind.ASK || kind == ItemKind.FAILURE

/** Send and Ask Agent take the reply box's text, so they need one. */
val InboxAction.needsText: Boolean get() = this == InboxAction.REPLY || this == InboxAction.ASK_AGENT

/**
 * How the item screen offers the answers (docs/inbox.md, "The item screen"; ADR 0040). Triage
 * ([done], [later] and the … [menu]) is the same on every item; the bottom answers the agent:
 * its [decisions], then one reply box that sends [text].
 */
data class ItemButtons(
    /** Under the body, in the host's order: Allow, Deny, Approve, Confirm, Reject, Retry. */
    val decisions: List<InboxAction>,
    /** What the reply box's text does: Send to the item's agent, or Ask Agent where it has no session (ADR 0041). */
    val text: InboxAction?,
    /** In the top bar's … menu: Complete Task, Drop Task. */
    val menu: List<InboxAction>,
    /** ✓: `keep-open` or `dismiss`, which close only the item. */
    val done: InboxAction?,
    /** Later moves the item down. */
    val later: Boolean,
    /** `choose` is offered: the prompt's options are the decisions. */
    val chooses: Boolean,
)

/** The item's answers by place, each in the host's order. Only actions the host listed appear. */
val InboxItem.buttons: ItemButtons
    get() = ItemButtons(
        decisions = answers.filter { it in DECISIONS },
        text = answers.firstOrNull { it.needsText },
        menu = answers.filter { it == InboxAction.ACCEPT || it == InboxAction.DROP },
        done = answers.firstOrNull { it == InboxAction.KEEP_OPEN || it == InboxAction.DISMISS },
        later = InboxAction.LATER in answers,
        chooses = InboxAction.CHOOSE in answers,
    )

private val DECISIONS = setOf(InboxAction.APPROVE, InboxAction.CONFIRM, InboxAction.REJECT, InboxAction.RETRY)

/** The options a `choose` answers with, by index; empty when the item takes no `choose`. */
val InboxItem.choices: List<ChatPrompt.Option>
    get() = if (InboxAction.CHOOSE in answers) prompt?.options.orEmpty() else emptyList()

/**
 * What Ctrl+Shift+Enter sends (ADR 0033 §4a.3): the reply box's action while it has [text], else
 * the host's first answer, unless that is `choose`, `later` or a text answer.
 */
fun InboxItem.mainAnswer(text: String = ""): InboxAction? {
    if (text.isNotBlank()) buttons.text?.let { return it }
    return answers.firstOrNull { it != InboxAction.READ }
        ?.takeIf { it != InboxAction.CHOOSE && it != InboxAction.LATER && !it.needsText }
}
