package tech.xdge.muxa.app

import android.graphics.Color
import android.graphics.drawable.ColorDrawable
import android.os.Bundle
import android.view.KeyEvent
import android.view.KeyboardShortcutGroup
import android.view.KeyboardShortcutInfo
import android.view.Menu
import androidx.activity.ComponentActivity
import androidx.activity.SystemBarStyle
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.ui.graphics.toArgb
import kotlinx.coroutines.launch
import tech.xdge.muxa.app.next.NextModel
import tech.xdge.muxa.app.next.NextRoot
import tech.xdge.muxa.app.ui.AppTheme
import tech.xdge.muxa.app.ui.MuxaTheme
import tech.xdge.muxa.app.ui.ThemeStore
import tech.xdge.muxa.session.ThemeToken

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
        follow(ThemeStore.current)
        windowId = app.windows.claim(savedInstanceState?.getString(KEY_WINDOW) ?: intent.getStringExtra(EXTRA_WINDOW))
        model = app.windows.model(windowId)
        // Debug builds only, for the UI tests: `adb shell am start ... --ez demo true`.
        if (BuildConfig.DEBUG && intent.getBooleanExtra(EXTRA_DEMO, false)) {
            model.startDemo()
        } else {
            // The app's start or a new window: connect to the host last used.
            model.scope.launch { model.resumeOrLastHost() }
        }
        setContent {
            // Automatic follows the system's mode while the app runs (ADR 0091 Amendment 1).
            val dark = isSystemInDarkTheme()
            LaunchedEffect(dark) { ThemeStore.follow(dark) }
            val theme = ThemeStore.current
            LaunchedEffect(theme) { follow(theme) }
            MuxaTheme { NextRoot(model) }
        }
    }

    /**
     * What Compose does not draw: the window behind it, seen during a resize, and the
     * system bars' icons, which follow the theme's appearance, not the system's.
     */
    private fun follow(theme: AppTheme) {
        window.setBackgroundDrawable(ColorDrawable(theme[ThemeToken.ground].toArgb()))
        val bars = if (theme.isDark) {
            SystemBarStyle.dark(Color.TRANSPARENT)
        } else {
            SystemBarStyle.light(Color.TRANSPARENT, Color.TRANSPARENT)
        }
        enableEdgeToEdge(bars, bars)
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
