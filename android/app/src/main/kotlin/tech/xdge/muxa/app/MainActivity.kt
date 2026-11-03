package tech.xdge.muxa.app

import android.os.Bundle
import android.view.KeyEvent
import android.view.KeyboardShortcutGroup
import android.view.KeyboardShortcutInfo
import android.view.Menu
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import kotlinx.coroutines.launch
import tech.xdge.muxa.demo.DemoBridge
import tech.xdge.muxa.app.ui.MuxaTheme
import tech.xdge.muxa.app.ui.Root
import tech.xdge.muxa.protocol.WORK_ASK
import tech.xdge.muxa.protocol.WORK_ROUTINES

/** One app window. Each window has its own [AppModel]: its connection, pane and keyboard (ADR 0029). */
class MainActivity : ComponentActivity() {
    private val app: MuxaApplication get() = application as MuxaApplication
    private lateinit var windowId: String
    private lateinit var model: AppModel

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        val requested = savedInstanceState?.getString(KEY_WINDOW) ?: intent.getStringExtra(EXTRA_WINDOW)
        // The process restarted: the window opens its own pane again, not the one last shown anywhere.
        val restarted = requested != null && !app.windows.has(requested)
        windowId = app.windows.claim(requested)
        model = app.windows.model(windowId)
        if (restarted && savedInstanceState != null) restore(savedInstanceState)
        // The system's "new window" action: the app is already in the foreground, so this
        // window connects on its own, to the host and pane last used.
        if (requested == null && windowId != Windows.MAIN) model.scope.launch { model.resume() }
        // Debug builds only, for the UI tests: `adb shell am start ... --ez demo true` starts
        // the demo on the test fixtures, and `--ez no-saved-hosts true` starts as a fresh install.
        if (BuildConfig.DEBUG && intent.getBooleanExtra(EXTRA_DEMO, false)) {
            val set = if (intent.getBooleanExtra(EXTRA_SHOWCASE, false)) DemoBridge.Content.SHOWCASE else DemoBridge.Content.TESTS
            model.startDemo(
                set,
                alerts = intent.getBooleanExtra(EXTRA_DEMO_ALERTS, false),
                taskRecords = !intent.getBooleanExtra(EXTRA_NO_TASK_RECORDS, false),
                taskInbox = !intent.getBooleanExtra(EXTRA_NO_TASK_INBOX, false),
                // A host from before ask sessions (ADR 0031).
                workLevel = if (intent.getBooleanExtra(EXTRA_WORK1, false)) WORK_ROUTINES else WORK_ASK,
            )
        } else if (BuildConfig.DEBUG && intent.getBooleanExtra(EXTRA_NO_SAVED_HOSTS, false)) {
            model.forgetSavedHosts()
        }
        setContent { MuxaTheme { Root(model) } }
    }

    override fun onSaveInstanceState(outState: Bundle) {
        super.onSaveInstanceState(outState)
        outState.putString(KEY_WINDOW, windowId)
        outState.putString(KEY_HOST, model.currentHostId)
        outState.putString(KEY_PANE, model.state.watchedPane)
    }

    private fun restore(saved: Bundle) {
        val host = model.hostList.hosts.firstOrNull { it.id == saved.getString(KEY_HOST) } ?: return
        model.begin(AppModel.WindowStart(host, saved.getString(KEY_PANE), demo = null))
    }

    override fun onDestroy() {
        app.windowClosed(windowId, finished = isFinishing && !isChangingConfigurations)
        super.onDestroy()
    }

    /** The app's shortcuts when nothing in the window has focus (Root takes them otherwise). */
    override fun onKeyDown(keyCode: Int, event: KeyEvent): Boolean = model.shortcut(event) || super.onKeyDown(keyCode, event)

    /** Listed in the system's shortcut helper (Meta+/). */
    override fun onProvideKeyboardShortcuts(data: MutableList<KeyboardShortcutGroup>, menu: Menu?, deviceId: Int) {
        val chord = KeyEvent.META_CTRL_ON or KeyEvent.META_SHIFT_ON
        fun item(label: String, key: Int) = KeyboardShortcutInfo(label, key, chord)
        data += KeyboardShortcutGroup(
            "Panes",
            listOf(
                item("Switch session", KeyEvent.KEYCODE_K),
                item("New task", KeyEvent.KEYCODE_N),
                item("Inbox", KeyEvent.KEYCODE_T),
                item("Agent / Terminal", KeyEvent.KEYCODE_J),
                item("Pretty / Raw", KeyEvent.KEYCODE_R),
                item("Documents", KeyEvent.KEYCODE_D),
                item("Previous session", KeyEvent.KEYCODE_LEFT_BRACKET),
                item("Next session", KeyEvent.KEYCODE_RIGHT_BRACKET),
                item("Session 1 to 9", KeyEvent.KEYCODE_1),
            ),
        )
        val moved = chord or KeyEvent.META_ALT_ON
        data += KeyboardShortcutGroup(
            "Inbox",
            listOf(
                item("Previous row", KeyEvent.KEYCODE_DPAD_UP),
                item("Next row", KeyEvent.KEYCODE_DPAD_DOWN),
                item("Open", KeyEvent.KEYCODE_O),
                item("Inbox / All Tasks", KeyEvent.KEYCODE_L),
                KeyboardShortcutInfo("Move down", KeyEvent.KEYCODE_DPAD_DOWN, moved),
                KeyboardShortcutInfo("Later", KeyEvent.KEYCODE_L, moved),
                item("Snooze (older hosts)", KeyEvent.KEYCODE_S),
            ),
        )
    }

    companion object {
        /** The window an Activity shows, from [openInNewWindow]. */
        const val EXTRA_WINDOW = "window"
        private const val KEY_WINDOW = "window"
        private const val KEY_HOST = "host"
        private const val KEY_PANE = "pane"
        const val EXTRA_DEMO = "demo"
        const val EXTRA_DEMO_ALERTS = "demo-alerts"
        const val EXTRA_SHOWCASE = "showcase"
        const val EXTRA_NO_SAVED_HOSTS = "no-saved-hosts"
        const val EXTRA_NO_TASK_RECORDS = "no-task-records"

        /** A bridge from before the shared Inbox order and Routines, for the local Snooze's test. */
        const val EXTRA_NO_TASK_INBOX = "no-task-inbox"

        /** A bridge from before ask sessions and branches named after the title. */
        const val EXTRA_WORK1 = "work1"
    }
}
