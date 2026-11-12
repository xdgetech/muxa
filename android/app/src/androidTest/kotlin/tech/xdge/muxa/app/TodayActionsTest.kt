package tech.xdge.muxa.app

import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.junit4.createEmptyComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performScrollToNode
import androidx.compose.ui.test.performTextInput
import androidx.test.core.app.ActivityScenario
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.After
import org.junit.Assert.assertFalse
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/**
 * Today rows say what to do (ADR 0059 §7) against the demo: the verb, Do, the primary action, Send that
 * names who gets the text, the secretary's Send at `work` level 17, and Re-rank with the runs left today.
 */
@OptIn(ExperimentalTestApi::class)
@RunWith(AndroidJUnit4::class)
class TodayActionsTest {
    @get:Rule
    val compose = createEmptyComposeRule()
    private var scenario: ActivityScenario<MainActivity>? = null
    private val model get() = ApplicationProvider.getApplicationContext<MuxaApplication>().model

    @After
    fun close() {
        scenario?.close()
    }

    private fun launch(intent: android.content.Intent = proposalsDemoIntent()) {
        scenario = ActivityScenario.launch(intent)
        compose.waitUntilAtLeastOneExists(hasTestTag("today-row-$PINNED"), TIMEOUT)
    }

    private fun waitForText(tag: String, text: String) = compose.waitForTextIn(tag, text, TIMEOUT)

    private fun details(target: String) {
        compose.onNodeWithTag("inbox-list").performScrollToNode(hasTestTag("today-why-$target"))
        compose.click("today-why-$target")
        compose.onNodeWithTag("inbox-list").performScrollToNode(hasTestTag("today-do-$target"))
    }

    @Test
    fun eachRowNamesItsAction() {
        launch()
        waitForText("today-title-$PINNED", "Start · ")
        waitForText("today-title-$DELEGATE", "Approve request · ")
        compose.onNodeWithTag("inbox-list").performScrollToNode(hasTestTag("today-row-$NEW_TASK"))
        waitForText("today-title-$NEW_TASK", "Start Task · ")
        waitForText("today-plan-now", "Re-rank")
        waitForText("today-runs", "2 of 3 runs left today")
    }

    /** Details leads with Do; the main button opens the target, and Send names the secretary. */
    @Test
    fun detailsLeadWithDoAndNameWhoGetsSend() {
        launch()
        details(NEW_TASK)
        waitForText("today-do-$NEW_TASK", "Do: Start the Task, or tell the secretary what to change.")
        waitForText("today-primary-$NEW_TASK", "Open the proposed Task")
        waitForText("today-send-$NEW_TASK", "Send to the secretary")
        compose.click("today-primary-$NEW_TASK")
        compose.waitUntil(TIMEOUT) { model.selectedItem == NEW_TASK }
    }

    /** A request item takes no text: the row says so, and offers no Send. */
    @Test
    fun anItemWithoutTextSaysSo() {
        launch()
        details(DELEGATE)
        compose.waitUntilAtLeastOneExists(hasTestTag("today-no-send-$DELEGATE"), TIMEOUT)
        assertFalse(compose.exists("today-send-$DELEGATE"))
    }

    /** A Work item's Send types into the secretary's root Ask session, and its Chat opens. */
    @Test
    fun aWorkItemsSendOpensTheSecretarysChat() {
        launch(todayDemoIntent())
        details(PINNED)
        waitForText("today-send-$PINNED", "Send to the secretary (secretary), with this Work item")
        compose.click("today-send-$PINNED")
        compose.onNodeWithTag("today-send-text").performTextInput("Ask the venues for May dates.")
        compose.click("today-send-confirm")
        compose.waitUntil(TIMEOUT) { model.paneShown && model.currentSession?.name == "ask-secretary" }
    }

    /** A level 16 host takes no text on `start-session`: a Work item's row offers no Send, and no runs show. */
    @Test
    fun aLevel16HostOffersNoSecretary() {
        launch(reviewDemoIntent())
        details(PINNED)
        compose.waitUntilAtLeastOneExists(hasTestTag("today-no-send-$PINNED"), TIMEOUT)
        assertFalse(compose.exists("today-send-$PINNED"))
        assertFalse(compose.exists("today-runs"))
    }

    private companion object {
        const val TIMEOUT = 15_000L
        const val PINNED = "w-0000b001"
        const val DELEGATE = "i-0000e002"
        const val NEW_TASK = "i-0000f301"
    }
}
