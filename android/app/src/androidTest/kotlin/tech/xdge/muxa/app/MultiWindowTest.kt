package tech.xdge.muxa.app

import android.app.Activity
import android.content.Intent
import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.junit4.createEmptyComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.test.core.app.ActivityScenario
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.runner.lifecycle.ActivityLifecycleMonitorRegistry
import androidx.test.runner.lifecycle.Stage
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotSame
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import tech.xdge.muxa.session.PaneSession

/**
 * Open in New Window gives a second window its own connection and pane (ADR 0029):
 * it moves to another pane without moving the first, and closing it ends its connection.
 */
@RunWith(AndroidJUnit4::class)
@OptIn(ExperimentalTestApi::class)
class MultiWindowTest {
    @get:Rule
    val compose = createEmptyComposeRule()
    private var scenario: ActivityScenario<MainActivity>? = null

    @After
    fun close() {
        others().forEach { InstrumentationRegistry.getInstrumentation().runOnMainSync { it.finish() } }
        scenario?.close()
    }

    /** Every muxa window but the one the test launched. */
    private fun others(): List<Activity> {
        var launched: Activity? = null
        scenario?.onActivity { launched = it }
        val found = mutableListOf<Activity>()
        InstrumentationRegistry.getInstrumentation().runOnMainSync {
            val monitor = ActivityLifecycleMonitorRegistry.getInstance()
            for (stage in listOf(Stage.RESUMED, Stage.PAUSED, Stage.STOPPED)) found += monitor.getActivitiesInStage(stage)
        }
        return found.filter { it is MainActivity && it !== launched }
    }

    @Test
    fun aSecondWindowWatchesItsOwnPane() {
        val app = ApplicationProvider.getApplicationContext<MuxaApplication>()
        scenario = ActivityScenario.launch(demoIntent())
        compose.waitForInitialPane()
        val main = app.model
        compose.waitUntil(TIMEOUT) { main.isConnected && main.state.watchedPane != null }
        val first = main.state.watchedPane

        compose.onNodeWithTag("more").performClick()
        compose.waitUntilAtLeastOneExists(hasTestTag("menu-new-window"), TIMEOUT)
        compose.onNodeWithTag("menu-new-window").performClick()
        compose.waitUntil(TIMEOUT) { app.windows.all.size == 2 && others().isNotEmpty() }
        val second = app.windows.all.last()
        compose.waitUntil(TIMEOUT) { second.isConnected && second.state.watchedPane == first && second.paneShown }
        assertNotSame("the windows share a connection", main.session, second.session)

        // Each window keeps its own sidebar mode (ADR 0029, 0035).
        InstrumentationRegistry.getInstrumentation().runOnMainSync { second.selectMode(SidebarMode.TASKS) }
        assertEquals(SidebarMode.TASKS, second.mode)
        assertEquals(SidebarMode.SESSIONS, main.mode)

        // The new window moves to another session; the first window stays on its pane.
        val other = second.visibleSessions.first { s -> s.panes.none { it.id == first } }
        InstrumentationRegistry.getInstrumentation().runOnMainSync { second.open(other) }
        compose.waitUntil(TIMEOUT) { second.state.watchedPane.let { it != null && it != first } }
        assertEquals(first, main.state.watchedPane)
        assertEquals(PaneSession.Phase.Connected, main.state.phase)

        // Closing the new window ends its connection and keeps the first one.
        others().forEach { InstrumentationRegistry.getInstrumentation().runOnMainSync { it.finish() } }
        compose.waitUntil(TIMEOUT) { app.windows.all.size == 1 && second.state.phase != PaneSession.Phase.Connected }
        assertEquals(first, main.state.watchedPane)
        assertEquals(PaneSession.Phase.Connected, main.state.phase)
    }

    private companion object {
        const val TIMEOUT = 10_000L
    }
}
