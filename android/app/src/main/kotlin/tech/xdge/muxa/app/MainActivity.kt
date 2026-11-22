package tech.xdge.muxa.app

import android.graphics.Color
import android.os.Bundle
import android.view.KeyEvent
import android.view.KeyboardShortcutGroup
import android.view.KeyboardShortcutInfo
import android.view.Menu
import androidx.activity.ComponentActivity
import androidx.activity.SystemBarStyle
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import kotlinx.coroutines.launch
import tech.xdge.muxa.app.next.NextModel
import tech.xdge.muxa.app.next.NextRoot
import tech.xdge.muxa.app.ui.MuxaTheme

/**
 * One app window on protocol v2: Home and Focus (ADR 0066, 0067). Each window has
 * its own [NextModel], so its own connection (ADR 0029).
 */
class MainActivity : ComponentActivity() {
    private val app: MuxaApplication get() = application as MuxaApplication
    private lateinit var windowId: String
    private lateinit var model: NextModel

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        // The design is dark only, so the bars keep light icons whatever the system setting.
        enableEdgeToEdge(SystemBarStyle.dark(Color.TRANSPARENT), SystemBarStyle.dark(Color.TRANSPARENT))
        windowId = app.windows.claim(savedInstanceState?.getString(KEY_WINDOW) ?: intent.getStringExtra(EXTRA_WINDOW))
        model = app.windows.model(windowId)
        // Debug builds only, for the UI tests: `adb shell am start ... --ez demo true`.
        if (BuildConfig.DEBUG && intent.getBooleanExtra(EXTRA_DEMO, false)) {
            model.startDemo()
        } else {
            // The app's start or a new window: connect to the host last used.
            model.scope.launch { model.resumeOrLastHost() }
        }
        setContent { MuxaTheme { NextRoot(model) } }
    }

    override fun onSaveInstanceState(outState: Bundle) {
        super.onSaveInstanceState(outState)
        outState.putString(KEY_WINDOW, windowId)
    }

    override fun onDestroy() {
        app.windowClosed(windowId, finished = isFinishing && !isChangingConfigurations)
        super.onDestroy()
    }

    /** Listed in the system's shortcut helper (Meta+/). */
    override fun onProvideKeyboardShortcuts(data: MutableList<KeyboardShortcutGroup>, menu: Menu?, deviceId: Int) {
        val ctrl = KeyEvent.META_CTRL_ON
        val chord = ctrl or KeyEvent.META_SHIFT_ON
        data += KeyboardShortcutGroup(
            "muxa",
            listOf(
                KeyboardShortcutInfo("Home", KeyEvent.KEYCODE_H, chord),
                KeyboardShortcutInfo("Focus", KeyEvent.KEYCODE_F, chord),
                KeyboardShortcutInfo("Capture", KeyEvent.KEYCODE_N, ctrl),
                KeyboardShortcutInfo("Done or Approve", KeyEvent.KEYCODE_ENTER, ctrl),
                KeyboardShortcutInfo("Later, tomorrow 9:00", KeyEvent.KEYCODE_L, ctrl),
                KeyboardShortcutInfo("Move up", KeyEvent.KEYCODE_DPAD_UP, KeyEvent.META_ALT_ON),
                KeyboardShortcutInfo("Move down", KeyEvent.KEYCODE_DPAD_DOWN, KeyEvent.META_ALT_ON),
            ),
        )
    }

    companion object {
        /** The window an Activity shows. */
        const val EXTRA_WINDOW = "window"
        private const val KEY_WINDOW = "window"
        const val EXTRA_DEMO = "demo"
    }
}
