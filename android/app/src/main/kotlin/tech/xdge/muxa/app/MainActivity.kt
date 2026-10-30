package tech.xdge.muxa.app

import android.os.Bundle
import android.view.KeyEvent
import android.view.KeyboardShortcutGroup
import android.view.KeyboardShortcutInfo
import android.view.Menu
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import tech.xdge.muxa.app.ui.MuxaTheme
import tech.xdge.muxa.app.ui.Root

class MainActivity : ComponentActivity() {
    private val model: AppModel get() = (application as MuxaApplication).model

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        // Debug builds only: `adb shell am start ... --ez demo true` uses the in-app bridge.
        if (BuildConfig.DEBUG && intent.getBooleanExtra(EXTRA_DEMO, false)) {
            model.startDemo(alerts = intent.getBooleanExtra(EXTRA_DEMO_ALERTS, false))
        }
        setContent { MuxaTheme { Root(model) } }
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
                item("Agent / Terminal", KeyEvent.KEYCODE_J),
                item("Pretty / Raw", KeyEvent.KEYCODE_R),
                item("Documents", KeyEvent.KEYCODE_D),
                item("Previous session", KeyEvent.KEYCODE_LEFT_BRACKET),
                item("Next session", KeyEvent.KEYCODE_RIGHT_BRACKET),
                item("Session 1 to 9", KeyEvent.KEYCODE_1),
            ),
        )
    }

    companion object {
        const val EXTRA_DEMO = "demo"
        const val EXTRA_DEMO_ALERTS = "demo-alerts"
    }
}
