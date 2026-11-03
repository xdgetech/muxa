package tech.xdge.muxa.app

import android.view.KeyEvent
import tech.xdge.muxa.session.InboxRow
import tech.xdge.muxa.session.hasKnownActions

/**
 * The app's shortcuts are Ctrl+Shift chords: plain Ctrl chords belong to the pane's
 * shell (ADR 0013). Every other key goes to the pane. True when [event] was one and it ran.
 */
fun AppModel.shortcut(event: KeyEvent): Boolean {
    if (event.action != KeyEvent.ACTION_DOWN || !event.isCtrlPressed || !event.isShiftPressed) return false
    if (event.keyCode == KeyEvent.KEYCODE_T) {
        if (showTasks) closeTasks() else showInbox()
        return true
    }
    if (showTasks) {
        when (event.keyCode) {
            KeyEvent.KEYCODE_N -> startNewTaskRecord()
            KeyEvent.KEYCODE_L -> if (event.isAltPressed) laterKey() else toggleInboxPage()
            else -> if (inboxPage == InboxPage.INBOX) inboxKey(event)
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

/**
 * The Inbox list's keys, as Command on iPad: Up/Down focus a row, O opens it, Alt+Down moves
 * a Task down, and S snoozes it on a host without the shared order.
 */
private fun AppModel.inboxKey(event: KeyEvent) {
    val row = focusedRow ?: return
    when (event.keyCode) {
        KeyEvent.KEYCODE_DPAD_DOWN -> if (event.isAltPressed) {
            if (row is InboxRow.Task && canPlaceTasks) moveDown(row)
        } else {
            stepInbox(1)
        }
        KeyEvent.KEYCODE_DPAD_UP -> stepInbox(-1)
        KeyEvent.KEYCODE_O -> act(row)
        KeyEvent.KEYCODE_S -> if (row is InboxRow.Task && !canPlaceTasks) snooze(row.item)
    }
}

/** Asks for Later on the focused row, where the host keeps it. */
private fun AppModel.laterKey() {
    if (inboxPage != InboxPage.INBOX) return
    when (val row = focusedRow) {
        is InboxRow.Task -> if (canPlaceTasks) inboxPrompt = InboxPrompt.Later(row)
        is InboxRow.Entry -> if (row.entry.hasKnownActions) inboxPrompt = InboxPrompt.Later(row)
        is InboxRow.Routine, null -> {}
    }
}
