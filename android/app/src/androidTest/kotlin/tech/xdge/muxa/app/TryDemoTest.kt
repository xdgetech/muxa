package tech.xdge.muxa.app

import android.content.Intent
import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.junit4.createEmptyComposeRule
import androidx.compose.ui.test.onAllNodesWithTag
import androidx.compose.ui.test.onFirst
import androidx.compose.ui.test.getUnclippedBoundsInRoot
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.height
import androidx.compose.ui.test.performClick
import androidx.test.core.app.ActivityScenario
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.After
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/** Try the Demo from a fresh install: the in-app bridge, and nothing kept after it. Ported from the iOS `TryDemoTests`. */
@OptIn(ExperimentalTestApi::class)
@RunWith(AndroidJUnit4::class)
class TryDemoTest {
    @get:Rule
    val compose = createEmptyComposeRule()
    private var scenario: ActivityScenario<MainActivity>? = null

    @After
    fun close() {
        scenario?.close()
    }

    @Test
    fun tryTheDemoAndLeaveIt() {
        val context = ApplicationProvider.getApplicationContext<MuxaApplication>()
        val intent = Intent(context, MainActivity::class.java).putExtra(MainActivity.EXTRA_NO_SAVED_HOSTS, true)
        scenario = ActivityScenario.launch(intent)

        // The list offers it, and so does the empty detail beside it on a tablet.
        compose.waitUntilAtLeastOneExists(hasTestTag("try-demo"), TIMEOUT)
        compose.onAllNodesWithTag("try-demo").onFirst().performClick()
        compose.waitUntilAtLeastOneExists(hasTestTag("composer"), TIMEOUT)
        // Its agent waits: the waiting row once grew over the messages and the composer on a phone.
        compose.waitUntilAtLeastOneExists(hasTestTag("waiting"), TIMEOUT)
        val waiting = compose.onNodeWithTag("waiting").getUnclippedBoundsInRoot().height
        val messages = compose.onNodeWithTag("messages").getUnclippedBoundsInRoot().height
        assertTrue("the waiting row is $waiting tall", waiting < 160.dp)
        assertTrue("the messages are $messages tall", messages > 120.dp)
        compose.onNodeWithTag("composer").assertIsDisplayed()

        // A phone pushed the pane over the list; a tablet hid the list beside it.
        val back = if (compose.onAllNodesWithTag("back").fetchSemanticsNodes().isNotEmpty()) "back" else "show-list"
        compose.onAllNodesWithTag(back).onFirst().performClick()
        compose.waitUntilAtLeastOneExists(hasTestTag("host-Demo"), TIMEOUT)
        compose.onAllNodesWithTag("leave-demo").onFirst().performClick()

        compose.waitUntilAtLeastOneExists(hasTestTag("try-demo"), TIMEOUT)
        assertFalse(compose.onAllNodesWithTag("host-Demo").fetchSemanticsNodes().isNotEmpty())
    }

    private companion object {
        const val TIMEOUT = 10_000L
    }
}
