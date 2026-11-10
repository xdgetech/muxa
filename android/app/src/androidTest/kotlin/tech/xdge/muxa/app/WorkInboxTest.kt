package tech.xdge.muxa.app

import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.junit4.createEmptyComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollToNode
import androidx.compose.ui.test.performTextInput
import androidx.compose.ui.test.performTouchInput
import androidx.compose.ui.test.swipeLeft
import androidx.compose.ui.test.swipeRight
import androidx.test.core.app.ActivityScenario
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import kotlinx.coroutines.runBlocking
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import tech.xdge.muxa.protocol.WorkState
import tech.xdge.muxa.session.listed

/** Capture and the Inbox's Work and Unsorted sections (ADR 0048), against the demo at `work` level 9. */
@OptIn(ExperimentalTestApi::class)
@RunWith(AndroidJUnit4::class)
class WorkInboxTest {
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

    private fun capture(text: String) {
        compose.click("sidebar-new-task")
        waitForTag("capture-text")
        compose.onNodeWithTag("capture-text").performTextInput(text)
    }

    /** Decisions, Work and Unsorted; the badge counts decisions only. */
    @Test
    fun theInboxHasThreeSectionsAndCountsDecisionsOnly() {
        launchOnInbox()
        for (section in listOf("decisions", "work", "unsorted")) scrollTo("inbox-section-$section")
        // Today comes with `work:10` only (ADR 0049).
        assertTrue(!compose.exists("inbox-section-today"))
        waitForText("sidebar-mode-inbox", "Inbox ${model.inboxItems.size}")
        // A Task with an open item: the Work item waits on it and has no answer of its own.
        scrollTo("work-row-$WAITING")
        waitForText("work-state-$WAITING", "waiting on decision")
        // Later moved it down; it is still listed.
        scrollTo("work-row-$LATER")
        waitForText("work-state-$LATER", "later")
    }

    @Test
    fun plusSavesACaptureUnderUnsorted() {
        launchOnInbox()
        capture("Call the venue about catering\nThey close at five.")
        compose.click("capture-save")
        compose.waitUntil(TIMEOUT) { model.state.captures.any { it.text.startsWith("Call the venue") } }
        val id = model.state.captures.first { it.text.startsWith("Call the venue") }.id
        scrollTo("capture-row-$id")
        waitForText("capture-row-$id", "Call the venue about catering")
    }

    @Test
    fun startATaskInsteadOpensNewTaskWithTheText() {
        launchOnInbox()
        capture("Fix the upload test\nIt fails on CI.")
        compose.click("capture-start-task")
        waitForText("task-title", "Fix the upload test")
        waitForText("task-goal", "It fails on CI.")
    }

    /** A swipe right completes the item; a swipe left moves it below the others (ADR 0040). */
    @Test
    fun swipesCompleteOrMoveAWorkItemDown() {
        launchOnInbox()
        compose.onNodeWithTag("work-row-$OPEN").performTouchInput { swipeLeft() }
        compose.waitUntil(TIMEOUT) { model.state.work.listed().last().id == OPEN }
        scrollTo("work-row-$OPEN")
        compose.onNodeWithTag("work-row-$OPEN").performTouchInput { swipeRight() }
        compose.waitUntil(TIMEOUT) { model.state.work.first { it.id == OPEN }.state == WorkState.DONE }
        compose.waitUntilDoesNotExist(hasTestTag("work-row-$OPEN"), TIMEOUT)
    }

    @Test
    fun anUnsortedCaptureBecomesAWorkItem() {
        launchOnInbox()
        scrollTo("capture-work-$FLAKY")
        compose.onNodeWithTag("capture-work-$FLAKY").performClick()
        compose.waitUntilDoesNotExist(hasTestTag("capture-row-$FLAKY"), TIMEOUT)
        val made = model.state.work.first { it.title == "Look into the flaky upload test" }
        scrollTo("work-row-${made.id}")
    }

    /** Start a Task from the Work item's detail: the Task is linked to the item. */
    @Test
    fun aTaskStartedFromAWorkItemIsLinkedToIt() {
        launchOnInbox()
        compose.click("work-row-$OPEN")
        compose.click("work-start-task")
        waitForText("task-title", "Plan the team offsite")
        compose.click("task-create")
        compose.waitUntil(TIMEOUT) { model.state.work.first { it.id == OPEN }.tasks.isNotEmpty() }
        val task = model.state.work.first { it.id == OPEN }.tasks.single()
        assertEquals(OPEN, model.state.tasks?.record(task)?.work)
    }

    /** A Save without a connection stays on the device as "Not sent", and goes on the next connect. */
    @Test
    fun anUnsentCaptureIsKeptAndSentOnConnect() {
        launchOnInbox()
        capture("Renew the passport")
        // The link drops while the sheet is open.
        runBlocking { model.session.disconnect() }
        compose.click("capture-save")
        waitForTag("capture-notice")
        assertEquals(listOf("Renew the passport"), model.unsentHere.map { it.text })
        compose.onNodeWithText("OK").performClick()
        model.connect(model.currentHostId!!)
        compose.waitUntil(TIMEOUT) { model.state.captures.any { it.text == "Renew the passport" } }
        compose.waitUntil(TIMEOUT) { model.unsentHere.isEmpty() }
        assertTrue(model.state.captures.count { it.text == "Renew the passport" } == 1)
    }

    private companion object {
        const val TIMEOUT = 10_000L
        const val OPEN = "w-0000b001"
        const val WAITING = "w-0000b002"
        const val LATER = "w-0000b003"
        const val FLAKY = "c-0000c001"
    }
}
