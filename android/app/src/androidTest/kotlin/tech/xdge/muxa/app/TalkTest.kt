package tech.xdge.muxa.app

import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.junit4.createEmptyComposeRule
import androidx.test.core.app.ActivityScenario
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/** Talk to Assistant (ADR 0054): the overall assistant's root Ask session opens on its Chat. */
@OptIn(ExperimentalTestApi::class)
@RunWith(AndroidJUnit4::class)
class TalkTest {
    @get:Rule
    val compose = createEmptyComposeRule()
    private var scenario: ActivityScenario<MainActivity>? = null
    private val model get() = ApplicationProvider.getApplicationContext<MuxaApplication>().model

    @After
    fun close() {
        scenario?.close()
    }

    private fun launch(talk: Boolean) {
        val intent = demoIntent(SidebarMode.INBOX)
            .putExtra(if (talk) MainActivity.EXTRA_WORK15 else MainActivity.EXTRA_WORK14, true)
        scenario = ActivityScenario.launch(intent)
        compose.waitUntilAtLeastOneExists(hasTestTag("inbox-section-today"), TIMEOUT)
    }

    /** The Ask session's pane opens, as after Ask Agent. */
    private fun opensTheSecretarysChat() {
        compose.waitUntil(TIMEOUT) { model.paneShown && model.currentSession?.name == "ask-$OVERALL" }
    }

    @Test
    fun todaysTalkOpensTheSecretarysChat() {
        launch(talk = true)
        compose.click("today-talk")
        opensTheSecretarysChat()
    }

    @Test
    fun thePlusSheetsTalkOpensTheSameSession() {
        launch(talk = true)
        compose.click("sidebar-new-task")
        compose.waitUntilAtLeastOneExists(hasTestTag("capture-talk"), TIMEOUT)
        compose.click("capture-talk")
        opensTheSecretarysChat()
        compose.runOnIdle { assertEquals(null, model.captureDraft) }
    }

    @Test
    fun aLevel14HostShowsNoTalk() {
        launch(talk = false)
        assertFalse(compose.exists("today-talk"))
        compose.click("sidebar-new-task")
        compose.waitUntilAtLeastOneExists(hasTestTag("capture-text"), TIMEOUT)
        assertFalse(compose.exists("capture-talk"))
    }

    private companion object {
        const val TIMEOUT = 10_000L
        const val OVERALL = "secretary"
    }
}
