package tech.xdge.muxa.session

import tech.xdge.muxa.protocol.ChatPrompt
import tech.xdge.muxa.protocol.InboxAction
import tech.xdge.muxa.protocol.InboxItem
import tech.xdge.muxa.protocol.ItemKind
import tech.xdge.muxa.protocol.ItemState

// The Inbox as a list of questions (ADR 0033): which items wait, and which answers the item
// screen shows where. The host decides what an item accepts (`answers`); this only places it.

/** Waits for the user: open, or back from Later ([now], epoch seconds). */
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

/** `redo` and `change-course` send the reply box's text, so they need one. */
val InboxAction.needsText: Boolean get() = this == InboxAction.REDO || this == InboxAction.CHANGE_COURSE

/** The answers the item screen shows as buttons, and the ones under More. */
data class ItemButtons(val main: List<InboxAction>, val more: List<InboxAction>)

/** The visible answers per kind (docs/inbox.md, "The item screen"); the rest the host accepts go under More. */
private fun shownFirst(kind: ItemKind): List<InboxAction> = when (kind) {
    ItemKind.ASK -> listOf(InboxAction.APPROVE, InboxAction.REJECT, InboxAction.DISMISS)
    ItemKind.REVIEW -> listOf(InboxAction.ACCEPT, InboxAction.REDO)
    ItemKind.FAILURE -> listOf(InboxAction.RETRY, InboxAction.CHANGE_COURSE)
    ItemKind.PROPOSAL -> listOf(InboxAction.APPROVE, InboxAction.REJECT)
    ItemKind.BRIEFING -> listOf(InboxAction.DISMISS)
    else -> emptyList()
}

/**
 * The item's buttons, in the host's order. `choose` is not a button: the prompt's options are.
 * Later always sits under More. An unknown kind shows its first two answers.
 */
val InboxItem.buttons: ItemButtons
    get() {
        val offered = answers.filter { it != InboxAction.CHOOSE && it != InboxAction.READ }
        val first = shownFirst(kind)
        val main = if (first.isEmpty()) offered.filter { it != InboxAction.LATER }.take(2) else offered.filter { it in first }
        return ItemButtons(main, offered - main.toSet())
    }

/** The options a `choose` answers with, by index; empty when the item takes no `choose`. */
val InboxItem.choices: List<ChatPrompt.Option>
    get() = if (InboxAction.CHOOSE in answers) prompt?.options.orEmpty() else emptyList()

/** What Ctrl+Shift+Enter sends: Approve, Accept, Retry or Done. Never one that needs a text. */
val InboxItem.mainAnswer: InboxAction? get() = buttons.main.firstOrNull { !it.needsText && it != InboxAction.REJECT }
