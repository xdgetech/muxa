package tech.xdge.muxa.app

import android.view.KeyEvent

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
            KeyEvent.KEYCODE_L -> { inboxList = !inboxList; selectedTask = null }
            KeyEvent.KEYCODE_DPAD_DOWN -> if (!inboxList && selectedTask == null) stepInbox(1)
            KeyEvent.KEYCODE_DPAD_UP -> if (!inboxList && selectedTask == null) stepInbox(-1)
            KeyEvent.KEYCODE_S -> if (!inboxList && selectedTask == null && !inboxEnded) snoozeInbox()
            KeyEvent.KEYCODE_O -> if (!inboxList && selectedTask == null && !inboxEnded) {
                val item = inboxQueue.firstOrNull { it.record.id == inboxFocus } ?: inboxQueue.firstOrNull()
                if (item != null) openTask(item.record, if (state.canChat && item.agentPane != null) AppModel.PaneView.CHAT else AppModel.PaneView.AGENT)
            }
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
