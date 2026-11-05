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

/** An agent is blocked on it, or it stopped: what a phone opens on after connecting. */
val InboxItem.isUrgent: Boolean get() = kind == ItemKind.ASK || kind == ItemKind.FAILURE

/** `reply`, `redo`, `change-course` and `ask-agent` send the reply box's text, so they need one. */
val InboxAction.needsText: Boolean
    get() = this == InboxAction.REPLY || this == InboxAction.REDO || this == InboxAction.CHANGE_COURSE || this == InboxAction.ASK_AGENT

/** The reply box only asks the workspace's agent (ADR 0041): no other answer takes its text. */
val InboxItem.onlyAsksAgent: Boolean
    get() = InboxAction.ASK_AGENT in answers && answers.none { it.needsText && it != InboxAction.ASK_AGENT }

/**
 * How the item screen offers the answers (docs/inbox.md, "The item screen"; ADR 0040): [main] text
 * buttons, [more] only when two or more would be hidden, [done] as ✓ and [later] as icon buttons.
 */
data class ItemButtons(
    val main: List<InboxAction>,
    val more: List<InboxAction>,
    /** ✓: `keep-open` or `dismiss`, which close only the item. */
    val done: InboxAction? = null,
    /** Later moves the item down. */
    val later: Boolean = false,
)

/**
 * The text buttons per kind; null for a kind this app does not know. A `review`'s Redo and
 * Change Course, and a `failure`'s Change Course and Drop Task, go under More. Ask Agent is a
 * button on a briefing and a proposal (ADR 0041); elsewhere it is hidden.
 */
private fun shownFirst(kind: ItemKind): Set<InboxAction>? = when (kind) {
    ItemKind.ASK -> setOf(InboxAction.APPROVE, InboxAction.REJECT, InboxAction.REPLY)
    ItemKind.REVIEW -> setOf(InboxAction.ACCEPT, InboxAction.REPLY)
    ItemKind.FAILURE -> setOf(InboxAction.RETRY, InboxAction.REPLY)
    ItemKind.PROPOSAL -> setOf(InboxAction.APPROVE, InboxAction.REJECT, InboxAction.ASK_AGENT)
    ItemKind.BRIEFING -> setOf(InboxAction.ASK_AGENT)
    else -> null
}

/**
 * The item's buttons, in the host's order. `choose` is not a button: the prompt's options are.
 * One answer that would be hidden is a button, not a More of one. An unknown kind shows its
 * first two answers.
 */
val InboxItem.buttons: ItemButtons
    get() {
        val done = answers.firstOrNull { it == InboxAction.KEEP_OPEN || it == InboxAction.DISMISS }
        val rest = answers.filter { it != InboxAction.CHOOSE && it != InboxAction.READ && it != InboxAction.LATER && it != done }
        val visible = shownFirst(kind) ?: rest.take(2).toSet()
        val hidden = rest.filter { it !in visible }
        val main = if (hidden.size >= 2) rest.filter { it in visible } else rest
        return ItemButtons(main, rest - main.toSet(), done, InboxAction.LATER in answers)
    }

/** The options a `choose` answers with, by index; empty when the item takes no `choose`. */
val InboxItem.choices: List<ChatPrompt.Option>
    get() = if (InboxAction.CHOOSE in answers) prompt?.options.orEmpty() else emptyList()

/**
 * What Ctrl+Shift+Enter sends (ADR 0033 §4a): `reply` while the reply box has [text], or
 * `ask-agent` when it is the only answer that takes a text (ADR 0041); else the
 * host's first answer, unless that is `choose`, `later` (it moves the item down) or one that needs a text.
 */
fun InboxItem.mainAnswer(text: String = ""): InboxAction? {
    if (text.isNotBlank() && InboxAction.REPLY in answers) return InboxAction.REPLY
    if (text.isNotBlank() && onlyAsksAgent) return InboxAction.ASK_AGENT
    return answers.firstOrNull()?.takeIf { it != InboxAction.CHOOSE && it != InboxAction.LATER && !it.needsText }
}
