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
    select(Selection.Item(id))
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
    if (!phoneLayout) openItem(next)
}

/** The focused item, else the first. */
val AppModel.focusedItem: InboxItem? get() = inboxItems.let { items -> items.firstOrNull { it.id == itemFocus } ?: items.firstOrNull() }

fun AppModel.reply(item: InboxItem): String = itemReplies[item.id].orEmpty()

fun AppModel.setReply(item: InboxItem, text: String) {
    itemReplies = itemReplies + (item.id to text)
}

/**
 * Sends [action]. Send takes the reply box's text. An answer returns to the
 * list on a phone; beside the list, the next item opens. An item answered while its pane is
 * shown leaves the list; the pane stays. Later moves the item down and opens the next one too
 * (ADR 0040).
 */
fun AppModel.answer(item: InboxItem, action: InboxAction, option: Int? = null) {
    val text = reply(item).trim().takeIf { action.needsText }
    if (action.needsText && text.isNullOrEmpty()) return
    sendAnswer(item, action, text, option)
}

/** Done and Drop that could lose work ask first ([AppModel.losingWork]); Remove sends them again with [forceWorktree]. */
internal fun AppModel.sendAnswer(item: InboxItem, action: InboxAction, text: String?, option: Int?, forceWorktree: Boolean = false) {
    if (answeringItem != null) return
    inboxPrompt = null
    val before = inboxItems.map { it.id }
    answeringItem = item.id
    scope.launch {
        try {
            session.answer(item, action, text, option, forceWorktree)
            itemReplies = itemReplies - item.id
            afterAnswer(item.id, before)
        } catch (e: PaneSession.Failure) {
            val reason = e.lostWork
            if (reason != null) losingWork = LosingWork.Answer(item, action, text, option, reason) else inboxError = PaneSession.describe(e)
        } finally {
            answeringItem = null
        }
    }
}

/** The item that took the answered one's place, beside the list; the list on a phone. */
private fun AppModel.afterAnswer(answered: String, before: List<String>) {
    if (selectedItem != answered || paneShown) return
    // A moved-down item is still listed; the next one takes its place all the same.
    val waiting = inboxItems.map { it.id }.toSet() - answered
    val next = before.dropWhile { it != answered }.drop(1).firstOrNull { it in waiting }
        ?: before.lastOrNull { it in waiting }
    if (phoneLayout || next == null) {
        selection = null
        if (phoneLayout) listShown = true
        itemFocus = next ?: inboxItems.firstOrNull()?.id
    } else {
        openItem(next)
    }
}

/**
 * Ask Agent (ADR 0041): the item and the reply box's text go to the workspace's Ask session, and
 * its Chat opens over the item. The item stays open; a failure lands in `inboxError`.
 */
fun AppModel.askAgent(item: InboxItem) {
    val text = reply(item).trim()
    if (text.isEmpty() || answeringItem != null) return
    answeringItem = item.id
    scope.launch {
        try {
            val started = session.askAgent(item, text)
            itemReplies = itemReplies - item.id
            val s = state.sessions.firstOrNull { it.id == started.session } ?: return@launch
            selection = Selection.Item(item.id)
            openPane(s, AppModel.PaneView.CHAT)
        } catch (e: PaneSession.Failure) {
            inboxError = PaneSession.describe(e)
        } finally {
            answeringItem = null
        }
    }
}

/** Approve on a proposal shows its action, target and content first; every other answer goes now. */
fun AppModel.tap(item: InboxItem, action: InboxAction) {
    if (action == InboxAction.APPROVE && item.kind == ItemKind.PROPOSAL && item.proposal != null) {
        inboxPrompt = InboxPrompt.ApproveItem(item)
    } else if (action == InboxAction.ASK_AGENT) {
        askAgent(item)
    } else {
        answer(item, action)
    }
}

/** Ctrl+Shift+Enter: the open item's main answer, else the focused one's. Send while its reply box has text. */
fun AppModel.sendMainAnswer() {
    val item = selectedItem?.let(::item) ?: focusedItem ?: return
    val action = item.mainAnswer(reply(item)) ?: return
    tap(item, action)
}

/** The item's Task session, at its agent pane; null when it has none any more. */
fun AppModel.itemSession(item: InboxItem) = state.sessions.firstOrNull { s ->
    item.pane != null && s.panes.any { it.id == item.pane }
} ?: item.task?.let { task -> state.sessions.firstOrNull { it.taskId == task } }

/**
 * The item's Chat or Terminal: its session's pane over the item, whose row stays highlighted.
 * Terminal is the agent's terminal; an item never opens Shell first (ADR 0035).
 */
fun AppModel.openItemSession(item: InboxItem, view: AppModel.PaneView) {
    val s = itemSession(item) ?: return
    selection = Selection.Item(item.id)
    openPane(s, view)
}

/** The item has a session whose agent has Chat. */
fun AppModel.itemHasChat(item: InboxItem): Boolean = itemSession(item)?.let(::canChat) == true
