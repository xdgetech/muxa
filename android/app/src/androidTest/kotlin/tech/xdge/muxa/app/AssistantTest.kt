package tech.xdge.muxa.app

import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.junit4.createEmptyComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performScrollToNode
import androidx.compose.ui.test.performTextInput
import androidx.test.core.app.ActivityScenario
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import tech.xdge.muxa.protocol.AssistScope
import tech.xdge.muxa.protocol.Delivery

/** Ask Assistant from Capture and from a Work item, and the Result in the detail (ADR 0048 §10), against the demo at level 9. */
@OptIn(ExperimentalTestApi::class)
@RunWith(AndroidJUnit4::class)
class AssistantTest {
    @get:Rule
    val compose = createEmptyComposeRule()
    private var scenario: ActivityScenario<MainActivity>? = null
    private val model get() = ApplicationProvider.getApplicationContext<MuxaApplication>().model

    @After
    fun close() {
        scenario?.close()
    }

    private fun launchOnInbox() {
        scenario = ActivityScenario.launch(demoIntent(SidebarMode.INBOX).putExtra(MainActivity.EXTRA_WORK9, true))
        waitForTag("work-row-$OPEN")
    }

    private fun waitForTag(tag: String) = compose.waitUntilAtLeastOneExists(hasTestTag(tag), TIMEOUT)

    private fun waitForText(tag: String, text: String) = compose.waitForTextIn(tag, text, TIMEOUT)

    private fun scrollTo(tag: String) {
        compose.onNodeWithTag("inbox-list").performScrollToNode(hasTestTag(tag))
        waitForTag(tag)
    }

    private fun detailScrollTo(tag: String) {
        compose.onNodeWithTag("work-detail").performScrollToNode(hasTestTag(tag))
        waitForTag(tag)
    }

    private fun requestOf(work: String) = model.state.requests.firstOrNull { it.work == work }

    /** The row says "Result ready"; the detail has the Captures' texts, the Result and the earlier one. */
    @Test
    fun theDetailShowsCaptureTextsAndTheResult() {
        launchOnInbox()
        scrollTo("work-row-$WITH_RESULT")
        waitForText("work-assist-$WITH_RESULT", "Result ready")
        compose.click("work-row-$WITH_RESULT")
        waitForText("work-capture-c-0000c004", "The registrar raised its price.")
        waitForText("request-delivery-$REQUEST", "result ready")
        detailScrollTo("result-$REQUEST-2")
        detailScrollTo("result-duplicate-$OPEN")
        detailScrollTo("result-earlier-$REQUEST-1")
        detailScrollTo("work-ask")
        waitForText("work-ask", "Ask again")
    }

    /** Ask Assistant: the delivery state shows, Open Session while the run lives, then the Result. */
    @Test
    fun askingFromTheDetailShowsTheDeliveryThenTheResult() {
        launchOnInbox()
        compose.click("work-row-$OPEN")
        waitForText("work-capture-c-0000c003", "Two days in May")
        detailScrollTo("ask-scope-compare")
        compose.click("ask-scope-compare")
        detailScrollTo("work-ask")
        compose.click("work-ask")
        compose.waitUntil(TIMEOUT) { requestOf(OPEN) != null }
        val q = requestOf(OPEN)!!
        assertEquals(AssistScope.COMPARE, q.scope)
        compose.waitUntil(TIMEOUT) { requestOf(OPEN)?.delivery == Delivery.RESULTED }
        waitForText("request-delivery-${q.id}", "result ready")
        detailScrollTo("result-${q.id}-1")
        compose.onNodeWithTag("work-detail").performScrollToNode(hasTestTag("work-ask"))
        waitForText("work-ask", "Ask again")
    }

    /** Cancel while the run waits or works; Open Session shows the run's pane. */
    @Test
    fun aRunningRequestCanBeOpenedOrCancelled() {
        launchOnInbox()
        compose.click("work-row-$OPEN")
        detailScrollTo("work-ask")
        compose.click("work-ask")
        compose.waitUntil(TIMEOUT) { requestOf(OPEN) != null }
        val q = requestOf(OPEN)!!
        detailScrollTo("request-cancel-${q.id}")
        compose.click("request-cancel-${q.id}")
        compose.waitUntil(TIMEOUT) { requestOf(OPEN)?.delivery == Delivery.CANCELLED }
        waitForText("request-delivery-${q.id}", "cancelled")
    }

    @Test
    fun openSessionShowsTheRunsPane() {
        launchOnInbox()
        compose.click("work-row-$OPEN")
        detailScrollTo("work-ask")
        compose.click("work-ask")
        compose.waitUntil(TIMEOUT) { requestOf(OPEN) != null }
        val q = requestOf(OPEN)!!
        detailScrollTo("request-session-${q.id}")
        compose.click("request-session-${q.id}")
        compose.waitUntil(TIMEOUT) { model.workDetail == null }
        val pane = model.state.sessions.first { it.taskId == q.run }.panes.first().id
        compose.waitUntil(TIMEOUT) { model.state.watchedPane == pane }
    }

    /** Ask Assistant on the Capture sheet needs a workspace; then the Capture becomes a Work item with a request. */
    @Test
    fun askAssistantFromCaptureMakesAWorkItemWithARequest() {
        launchOnInbox()
        compose.click("sidebar-new-task")
        waitForTag("capture-text")
        compose.onNodeWithTag("capture-text").performTextInput("Compare error trackers\nSentry or Rollbar.")
        compose.onNodeWithTag("capture-ask").assertIsNotEnabled()
        compose.click("capture-workspace")
        compose.click("capture-workspace-work")
        compose.click("capture-scope-compare")
        compose.click("capture-ask")
        compose.waitUntil(TIMEOUT) {
            val made = model.state.work.firstOrNull { it.title == "Compare error trackers" }
            made != null && requestOf(made.id)?.scope == AssistScope.COMPARE
        }
        compose.waitUntilDoesNotExist(hasTestTag("capture-text"), TIMEOUT)
    }

    private companion object {
        const val TIMEOUT = 10_000L
        const val OPEN = "w-0000b001"
        const val WITH_RESULT = "w-0000b003"
        const val REQUEST = "q-0000a001"
    }
}
