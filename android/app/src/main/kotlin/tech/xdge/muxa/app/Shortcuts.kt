package tech.xdge.muxa.app

import android.view.KeyEvent
import tech.xdge.muxa.protocol.InboxAction

/**
 * The app's shortcuts are Ctrl+Shift chords: plain Ctrl chords belong to the pane's
 * shell (ADR 0013). Every other key goes to the pane. True when [event] was one and it ran.
 */
fun AppModel.shortcut(event: KeyEvent): Boolean {
    if (openFocusedItem(event)) return true
    if (event.action != KeyEvent.ACTION_DOWN || !event.isCtrlPressed || !event.isShiftPressed) return false
    if (event.keyCode == KeyEvent.KEYCODE_T) {
        if (showTasks) closeTasks() else showInbox()
        return true
    }
    if (showTasks) {
        when (event.keyCode) {
            KeyEvent.KEYCODE_N -> startNewTaskRecord()
            KeyEvent.KEYCODE_L -> if (event.isAltPressed) laterKey() else toggleInboxPage()
            else -> if (inboxPage == InboxPage.INBOX) itemKey(event)
        }
        return true
    }
    when (event.keyCode) {
        KeyEvent.KEYCODE_K -> showSwitcher = true
        KeyEvent.KEYCODE_N -> startNewTask()
        KeyEvent.KEYCODE_J -> toggleAgentTerminal()
        KeyEvent.KEYCODE_R -> setPretty(!showsPretty)
        KeyEvent.KEYCODE_D -> openReader()
        KeyEvent.KEYCODE_LEFT_BRACKET -> step(-1)
        KeyEvent.KEYCODE_RIGHT_BRACKET -> step(1)
        in KeyEvent.KEYCODE_1..KeyEvent.KEYCODE_9 -> openSession(event.keyCode - KeyEvent.KEYCODE_0)
        else -> return false
    }
    return true
}

/** Inbox and All Tasks; Routines count as All Tasks. */
private fun AppModel.toggleInboxPage() {
    inboxPage = if (inboxPage == InboxPage.INBOX) InboxPage.TASKS else InboxPage.INBOX
    clearInboxSelection()
}

/** The Inbox items' keys (ADR 0033): Up/Down move, O opens, Enter sends the main answer. */
private fun AppModel.itemKey(event: KeyEvent) {
    when (event.keyCode) {
        KeyEvent.KEYCODE_DPAD_DOWN -> stepItems(1)
        KeyEvent.KEYCODE_DPAD_UP -> stepItems(-1)
        KeyEvent.KEYCODE_O -> focusedItem?.let { openItem(it.id) }
        KeyEvent.KEYCODE_ENTER, KeyEvent.KEYCODE_NUMPAD_ENTER -> sendMainAnswer()
    }
}

/**
 * A plain Enter on the Inbox list opens the focused item. Once it is open, Enter belongs to
 * the reply box.
 */
private fun AppModel.openFocusedItem(event: KeyEvent): Boolean {
    if (!showTasks || !usesItems || inboxPage != InboxPage.INBOX || event.isCtrlPressed || event.isShiftPressed || event.isAltPressed) return false
    if (event.keyCode != KeyEvent.KEYCODE_ENTER && event.keyCode != KeyEvent.KEYCODE_NUMPAD_ENTER) return false
    val item = focusedItem ?: return false
    if (item.id == selectedItem) return false
    if (event.action == KeyEvent.ACTION_DOWN) openItem(item.id)
    return true
}

/** Asks for Later on the open item, else the focused one. */
private fun AppModel.laterKey() {
    if (inboxPage != InboxPage.INBOX) return
    val item = selectedItem?.let(::item) ?: focusedItem
    if (item != null && InboxAction.LATER in item.answers) inboxPrompt = InboxPrompt.ItemLater(item)
}
