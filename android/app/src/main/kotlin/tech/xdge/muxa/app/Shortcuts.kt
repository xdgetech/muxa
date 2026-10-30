package tech.xdge.muxa.app

import android.view.KeyEvent

/**
 * The app's shortcuts are Ctrl+Shift chords: plain Ctrl chords belong to the pane's
 * shell (ADR 0013). Every other key goes to the pane. True when [event] was one and it ran.
 */
fun AppModel.shortcut(event: KeyEvent): Boolean {
    if (event.action != KeyEvent.ACTION_DOWN || !event.isCtrlPressed || !event.isShiftPressed) return false
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
