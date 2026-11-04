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
    if (modeKey(event)) return true
    if (itemsShown && itemKey(event)) return true
    when (event.keyCode) {
        KeyEvent.KEYCODE_K -> showSwitcher = true
        KeyEvent.KEYCODE_N -> startNewTask()
        KeyEvent.KEYCODE_J -> toggleAgentTerminal()
        KeyEvent.KEYCODE_R -> setPretty(!showsPretty)
        KeyEvent.KEYCODE_D -> openReader()
        KeyEvent.KEYCODE_LEFT_BRACKET -> step(-1)
        KeyEvent.KEYCODE_RIGHT_BRACKET -> step(1)
        // KEYCODE_0 sits just below this range, so Ctrl+Shift+0 stays the sidebar's key.
        in KeyEvent.KEYCODE_1..KeyEvent.KEYCODE_9 -> openSession(event.keyCode - KeyEvent.KEYCODE_0)
        else -> return false
    }
    return true
}

/**
 * The sidebar's keys (ADR 0035): T Inbox, L Tasks, S Sessions, each with the sidebar shown,
 * and 0 shows or hides it. Ctrl+Shift+Alt+L is Later, not Tasks.
 */
private fun AppModel.modeKey(event: KeyEvent): Boolean {
    if (!isConnected && event.keyCode != KeyEvent.KEYCODE_0) return false
    when (event.keyCode) {
        KeyEvent.KEYCODE_T -> showMode(SidebarMode.INBOX)
        KeyEvent.KEYCODE_L -> if (event.isAltPressed) laterKey() else showMode(SidebarMode.TASKS)
        KeyEvent.KEYCODE_S -> showMode(SidebarMode.SESSIONS)
        KeyEvent.KEYCODE_0 -> toggleList()
        else -> return false
    }
    return true
}

/** Ctrl+Shift+0. On a phone the list and the detail take turns, so it goes back to the list. */
private fun AppModel.toggleList() {
    if (phoneLayout) {
        if (!listShown) back()
    } else {
        listShown = !listShown
    }
}

/** The Inbox items are on screen: Inbox mode, with no pane over its detail. */
private val AppModel.itemsShown: Boolean get() = isConnected && usesItems && mode == SidebarMode.INBOX && !paneShown

/**
 * The Inbox items' keys (ADR 0033, 0035): Up/Down move, O opens the item's terminal, Enter
 * sends the main answer.
 */
private fun AppModel.itemKey(event: KeyEvent): Boolean {
    when (event.keyCode) {
        KeyEvent.KEYCODE_DPAD_DOWN -> stepItems(1)
        KeyEvent.KEYCODE_DPAD_UP -> stepItems(-1)
        KeyEvent.KEYCODE_O -> (selectedItem?.let(::item) ?: focusedItem)?.let { openItemSession(it, AppModel.PaneView.AGENT) }
        KeyEvent.KEYCODE_ENTER, KeyEvent.KEYCODE_NUMPAD_ENTER -> sendMainAnswer()
        else -> return false
    }
    return true
}

/**
 * A plain Enter on the Inbox list opens the focused item. Once it is open, Enter belongs to
 * the reply box.
 */
private fun AppModel.openFocusedItem(event: KeyEvent): Boolean {
    if (!itemsShown || event.isCtrlPressed || event.isShiftPressed || event.isAltPressed) return false
    if (event.keyCode != KeyEvent.KEYCODE_ENTER && event.keyCode != KeyEvent.KEYCODE_NUMPAD_ENTER) return false
    val item = focusedItem ?: return false
    if (item.id == selectedItem) return false
    if (event.action == KeyEvent.ACTION_DOWN) openItem(item.id)
    return true
}

/** Asks for Later on the open item, else the focused one. */
private fun AppModel.laterKey() {
    if (!itemsShown) return
    val item = selectedItem?.let(::item) ?: focusedItem
    if (item != null && InboxAction.LATER in item.answers) inboxPrompt = InboxPrompt.ItemLater(item)
}
