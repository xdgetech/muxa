package tech.xdge.muxa.app

import kotlinx.coroutines.launch
import tech.xdge.muxa.protocol.InboxAction
import tech.xdge.muxa.protocol.InboxItem
import tech.xdge.muxa.protocol.ItemKind
import tech.xdge.muxa.session.PaneSession
import tech.xdge.muxa.session.mainAnswer
import tech.xdge.muxa.session.needsText

// The Inbox as a list of questions (ADR 0033). Every answer is written on the host; a failure
// lands in `inboxError` with the host's reason, and the list is read again.

fun AppModel.item(id: String): InboxItem? = state.inboxItems.firstOrNull { it.id == id }

/** Opens the item screen on [id], and marks the item read on every device. */
fun AppModel.openItem(id: String) {
    clearInboxSelection()
    selectedItem = id
    itemFocus = id
    val item = item(id) ?: return
    scope.launch { session.markRead(item) }
}

/** Moves the focus [offset] items, without wrapping. Beside the list, the item opens too. */
fun AppModel.stepItems(offset: Int) {
    val items = inboxItems
    if (items.isEmpty()) return
    val index = items.indexOfFirst { it.id == itemFocus }.coerceAtLeast(0)
    val next = items[(index + offset).coerceIn(0, items.lastIndex)].id
    itemFocus = next
    if (!phoneInbox) openItem(next)
}

/** The focused item, else the first. */
val AppModel.focusedItem: InboxItem? get() = inboxItems.let { items -> items.firstOrNull { it.id == itemFocus } ?: items.firstOrNull() }

fun AppModel.reply(item: InboxItem): String = itemReplies[item.id].orEmpty()

fun AppModel.setReply(item: InboxItem, text: String) {
    itemReplies = itemReplies + (item.id to text)
}

/**
 * Sends [action]. Redo and Change course take the reply box's text. An answer returns to the
 * list on a phone; beside the list, the next item opens.
 */
fun AppModel.answer(item: InboxItem, action: InboxAction, option: Int? = null, until: Long? = null) {
    val text = reply(item).trim().takeIf { action.needsText }
    if (action.needsText && text.isNullOrEmpty()) return
    if (answeringItem != null) return
    inboxPrompt = null
    val before = inboxItems.map { it.id }
    answeringItem = item.id
    scope.launch {
        try {
            session.answer(item, action, text, option, until)
            itemReplies = itemReplies - item.id
            afterAnswer(item.id, before)
        } catch (e: PaneSession.Failure) {
            inboxError = PaneSession.describe(e)
        } finally {
            answeringItem = null
        }
    }
}

/** The item that took the answered one's place, beside the list; the list on a phone. */
private fun AppModel.afterAnswer(answered: String, before: List<String>) {
    if (selectedItem != answered) return
    val waiting = inboxItems.map { it.id }.toSet()
    val next = before.dropWhile { it != answered }.drop(1).firstOrNull { it in waiting }
        ?: before.lastOrNull { it in waiting }
    if (phoneInbox || next == null) {
        selectedItem = null
        itemFocus = next ?: inboxItems.firstOrNull()?.id
    } else {
        openItem(next)
    }
}

/** Approve on a proposal shows its action, target and content first; every other answer goes now. */
fun AppModel.tap(item: InboxItem, action: InboxAction) {
    if (action == InboxAction.APPROVE && item.kind == ItemKind.PROPOSAL && item.proposal != null) {
        inboxPrompt = InboxPrompt.ApproveItem(item)
    } else if (action == InboxAction.LATER) {
        inboxPrompt = InboxPrompt.ItemLater(item)
    } else {
        answer(item, action)
    }
}

/** Ctrl+Shift+Enter: the open item's main answer, else the focused one's. */
fun AppModel.sendMainAnswer() {
    val item = selectedItem?.let(::item) ?: focusedItem ?: return
    val action = item.mainAnswer ?: return
    tap(item, action)
}

/** The item's Task session, at its agent pane; null when it has none any more. */
fun AppModel.itemSession(item: InboxItem) = state.sessions.firstOrNull { s ->
    item.pane != null && s.panes.any { it.id == item.pane }
} ?: item.task?.let { task -> state.sessions.firstOrNull { it.taskId == task } }

/** Open Terminal: leaves the Inbox for the item's session. */
fun AppModel.openItemTerminal(item: InboxItem) {
    itemSession(item)?.let { openTerminal(it) }
}
