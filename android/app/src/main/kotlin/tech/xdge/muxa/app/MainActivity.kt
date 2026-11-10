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
import tech.xdge.muxa.protocol.WORK_ASSISTANT
import tech.xdge.muxa.protocol.WORK_DELETE_INBOX
import tech.xdge.muxa.protocol.WORK_PLANS
import tech.xdge.muxa.protocol.WORK_POLICIES
import tech.xdge.muxa.protocol.WORK_WORKTREE_END
import tech.xdge.muxa.protocol.WORK_ITEMS
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
        // A debug launch starts in the mode it names, else in Inbox mode, whatever the device remembers.
        if (BuildConfig.DEBUG && (intent.hasExtra(EXTRA_DEMO) || intent.hasExtra(EXTRA_NO_SAVED_HOSTS))) {
            model.mode = SidebarMode.of(intent.getStringExtra(EXTRA_MODE)) ?: SidebarMode.INBOX
        }
        if (BuildConfig.DEBUG && intent.getBooleanExtra(EXTRA_DEMO, false)) {
            val set = if (intent.getBooleanExtra(EXTRA_SHOWCASE, false)) DemoBridge.Content.SHOWCASE else DemoBridge.Content.TESTS
            model.startDemo(
                set,
                alerts = intent.getBooleanExtra(EXTRA_DEMO_ALERTS, false),
                taskRecords = !intent.getBooleanExtra(EXTRA_NO_TASK_RECORDS, false),
                // A host from before ask sessions (ADR 0031), Inbox items (ADR 0033) or
                // closing ask sessions (ADR 0039), or one with Captures and Work items (ADR 0048),
                // with the overall assistant's plans (ADR 0049), or with policies (ADR 0050).
                workLevel = when {
                    intent.getBooleanExtra(EXTRA_WORK11, false) -> WORK_POLICIES
                    intent.getBooleanExtra(EXTRA_WORK10, false) -> WORK_PLANS
                    intent.getBooleanExtra(EXTRA_WORK9, false) -> WORK_ASSISTANT
                    intent.getBooleanExtra(EXTRA_WORK1, false) -> WORK_ROUTINES
                    intent.getBooleanExtra(EXTRA_WORK2, false) -> WORK_ASK
                    intent.getBooleanExtra(EXTRA_WORK3, false) -> WORK_ITEMS
                    else -> WORK_DELETE_INBOX
                },
                oldItems = intent.getBooleanExtra(EXTRA_OLD_ITEMS, false),
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
        outState.putString(KEY_PANE, model.state.watchedPane.takeIf { model.paneShown })
        outState.putString(KEY_MODE, model.mode.tag)
    }

    private fun restore(saved: Bundle) {
        SidebarMode.of(saved.getString(KEY_MODE))?.let { model.mode = it }
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
                item("Main answer", KeyEvent.KEYCODE_ENTER),
                item("Inbox / All Tasks", KeyEvent.KEYCODE_L),
                KeyboardShortcutInfo("Later", KeyEvent.KEYCODE_L, moved),
            ),
        )
    }

    companion object {
        /** The window an Activity shows, from [openInNewWindow]. */
        const val EXTRA_WINDOW = "window"
        private const val KEY_WINDOW = "window"
        private const val KEY_HOST = "host"
        private const val KEY_PANE = "pane"
        private const val KEY_MODE = "mode"
        const val EXTRA_DEMO = "demo"
        const val EXTRA_DEMO_ALERTS = "demo-alerts"
        const val EXTRA_SHOWCASE = "showcase"
        const val EXTRA_NO_SAVED_HOSTS = "no-saved-hosts"

        /** The sidebar mode a debug launch starts in: `inbox`, `tasks` or `sessions` (ADR 0035). */
        const val EXTRA_MODE = "mode"
        const val EXTRA_NO_TASK_RECORDS = "no-task-records"

        /** A bridge from before ask sessions and branches named after the title. */
        const val EXTRA_WORK1 = "work1"

        /** A bridge from before Inbox items: the Inbox says the host needs a newer muxa-bridge. */
        const val EXTRA_WORK2 = "work2"

        /** A bridge from before closing ask sessions: no Close Session. */
        const val EXTRA_WORK3 = "work3"

        /** A bridge with Captures and Work items: `+` opens Capture, and the Inbox has three sections. */
        const val EXTRA_WORK9 = "work9"

        /** A bridge with the overall assistant: Today in the Inbox, pins, deadlines and notes. */
        const val EXTRA_WORK10 = "work10"

        /** A bridge with policies, corrections and hypotheses: Policies, Today's reorder and Not today. */
        const val EXTRA_WORK11 = "work11"

        /** The failure and the last proposal are four days old, for Delete Old (ADR 0046). */
        const val EXTRA_OLD_ITEMS = "old-items"
    }
}
