package tech.xdge.muxa.app

import android.content.Intent
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.uiautomator.By
import androidx.test.uiautomator.BySelector
import androidx.test.uiautomator.UiDevice
import androidx.test.uiautomator.UiObject2
import androidx.test.uiautomator.Until
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

/**
 * The minified release build connects over SSH, starts the bridge and shows a pane.
 * It drives the app as a user does, through the accessibility tree, so it relies on
 * no class or member name that R8 renames. Run it with `make test-android-release`:
 * that script starts the throwaway SSH host and tmux server, and passes their details.
 */
@RunWith(AndroidJUnit4::class)
class ReleaseSshTest {
    private val instrumentation = InstrumentationRegistry.getInstrumentation()
    private val device = UiDevice.getInstance(instrumentation)

    @Test
    fun connectsOverSshAndShowsThePane() {
        val args = InstrumentationRegistry.getArguments()
        val port = requireNotNull(args.getString("muxaSshPort")) { "run it with make test-android-release" }
        val marker = requireNotNull(args.getString("muxaMarker")) { "run it with make test-android-release" }
        assertTrue("the release test needs the release build", !BuildConfig.DEBUG)

        val context = instrumentation.targetContext
        val launch = requireNotNull(context.packageManager.getLaunchIntentForPackage(context.packageName))
        context.startActivity(launch.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TASK))

        // A phone shows the host list's icon, a tablet also the empty detail's button.
        await(By.desc("Add Host")).click()
        field("host-address", 1).text = HOST
        field("host-user", 2).text = USER
        await(By.clazz(EDIT_TEXT).text("22")).text = port
        await(By.res("host-connect"), By.text("Connect")).click()

        // First use: the host key is new, so the app asks.
        await(By.text("Trust")).click()

        // The bridge sends the pane's screen; the pane view reports it as its text.
        val pane = device.wait(Until.findObject(By.textContains(marker)), CONNECT_TIMEOUT)
        assertNotNull("the pane never showed \"$marker\"", pane)
    }

    /** The field tagged [tag], or the [index]th text field when the dialog has no tag ids. */
    private fun field(tag: String, index: Int): UiObject2 {
        await(By.clazz(EDIT_TEXT))
        device.findObject(By.res(tag))?.let { return it }
        return device.findObjects(By.clazz(EDIT_TEXT)).sortedBy { it.visibleBounds.top }[index]
    }

    private fun await(vararg selectors: BySelector): UiObject2 {
        val deadline = System.currentTimeMillis() + UI_TIMEOUT
        while (System.currentTimeMillis() < deadline) {
            selectors.firstNotNullOfOrNull { device.findObject(it) }?.let { return it }
            device.waitForIdle(POLL)
        }
        throw AssertionError("not shown: ${selectors.joinToString()}")
    }

    private companion object {
        // `adb reverse` forwards the device's port to the throwaway host on the computer.
        const val HOST = "127.0.0.1"
        const val USER = "muxa"
        const val EDIT_TEXT = "android.widget.EditText"
        const val UI_TIMEOUT = 15_000L
        const val CONNECT_TIMEOUT = 60_000L
        const val POLL = 250L
    }
}
