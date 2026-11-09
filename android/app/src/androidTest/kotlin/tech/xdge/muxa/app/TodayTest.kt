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
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import tech.xdge.muxa.app.ui.wire
import tech.xdge.muxa.protocol.InboxAction
import tech.xdge.muxa.session.listed
import tech.xdge.muxa.session.shown

/** Today, the overall assistant's items, pins and notes (ADR 0047 §5, §8), against the demo at `work` level 9. */
@OptIn(ExperimentalTestApi::class)
@RunWith(AndroidJUnit4::class)
class TodayTest {
    @get:Rule
    val compose = createEmptyComposeRule()
    private var scenario: ActivityScenario<MainActivity>? = null
    private val model get() = ApplicationProvider.getApplicationContext<MuxaApplication>().model

    @After
    fun close() {
        scenario?.close()
    }

    private fun launchOnInbox() {
        scenario = ActivityScenario.launch(planDemoIntent())
        waitForTag("today-row-$PINNED")
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

    /** The plan's rows lead the Inbox, with their minutes, the host's marks and Why; a row opens its target. */
    @Test
    fun todayShowsThePlanAboveDecisions() {
        launchOnInbox()
        waitForTag("inbox-section-today")
        waitForText("today-row-$PINNED", "Pinned by you")
        waitForText("today-minutes-$PINNED", "30–60 min")
        waitForTag("today-row-$REVIEW")
        waitForText("today-due-soon", "Due soon, moved down by you: Renew the domain")
        assertFalse(compose.exists("today-new"))
        compose.click("today-why-$PINNED")
        waitForText("today-why-text-$PINNED", "Would help: Which dates the team prefers")
        compose.click("today-open-$WAITING")
        compose.waitUntil(TIMEOUT) { model.workDetail == WAITING }
        waitForText("work-detail-today", "In Today: Its Task waits on your decision.")
    }

    /** Plan now: the newer plan shows New, and the selection and the focused row stay. */
    @Test
    fun planNowShowsANewPlanAndKeepsTheSelection() {
        launchOnInbox()
        compose.runOnIdle { model.itemFocus = FAILURE }
        val selection = model.selection
        val first = model.state.plans.shown()!!.id
        compose.click("today-plan-now")
        compose.onNodeWithTag("plan-now-minutes").performTextInput("45")
        compose.click("plan-now-confirm")
        compose.waitUntil(TIMEOUT) { model.state.plans.firstOrNull()?.minutes == 45 }
        waitForTag("today-new")
        assertTrue(model.state.plans.shown()!!.id != first)
        assertEquals(FAILURE, model.itemFocus)
        assertEquals(selection, model.selection)
        compose.click("today-open-$PINNED")
        compose.waitUntilDoesNotExist(hasTestTag("today-new"), TIMEOUT)
    }

    /** Approve asks the receiving workspace once; the item leaves the Inbox. */
    @Test
    fun approvingADelegateAsksOnce() {
        launchOnInbox()
        scrollTo("item-row-$DELEGATE")
        compose.click("item-row-$DELEGATE")
        waitForTag("item-delegate")
        waitForText("item-detail", "personal's assistant")
        compose.click("item-${InboxAction.APPROVE.wire}")
        compose.waitUntil(TIMEOUT) { model.inboxItems.none { it.id == DELEGATE } }
        val asked = model.state.requests.filter { it.work == PINNED }
        assertEquals(listOf("personal"), asked.map { it.workspace })
    }

    @Test
    fun rejectingASortKeepsTheCaptureUnsorted() {
        launchOnInbox()
        scrollTo("item-row-$SORT")
        compose.click("item-row-$SORT")
        waitForText("item-sort", "Proposed sort")
        compose.click("item-${InboxAction.REJECT.wire}")
        compose.waitUntil(TIMEOUT) { model.inboxItems.none { it.id == SORT } }
        assertTrue(model.state.captures.any { it.id == FLAKY })
    }

    /** Pin lists a moved-down item with the pinned ones, before the others. */
    @Test
    fun pinMovesAnItemFirst() {
        launchOnInbox()
        scrollTo("work-row-$LATER")
        compose.click("work-row-$LATER")
        detailScrollTo("work-pin")
        compose.click("work-pin")
        compose.waitUntil(TIMEOUT) { model.state.work.listed().map { it.id }.take(2) == listOf(PINNED, LATER) }
        waitForText("work-detail-state", "pinned")
    }

    /** Update notes writes the Result's proposal on the host once. */
    @Test
    fun updateNotesAppliesTheProposal() {
        launchOnInbox()
        scrollTo("work-row-$LATER")
        compose.click("work-row-$LATER")
        detailScrollTo("work-notes-text")
        waitForText("work-notes-text", "auto-renew is the default")
        detailScrollTo("work-notes-apply")
        compose.click("work-notes-apply")
        waitForTag("work-notes-applied")
        compose.waitUntil(TIMEOUT) { model.state.assistant?.delegates?.first { it.workspace == "personal" }?.notesAt != null }
    }

    /** The overall assistant's workspace is in the host's list, but not in Capture's or New Task's picker. */
    @Test
    fun theOverallWorkspaceIsNotInThePickers() {
        launchOnInbox()
        compose.click("sidebar-new-task")
        waitForTag("capture-text")
        compose.onNodeWithTag("capture-text").performTextInput("Book the train")
        compose.waitUntil(TIMEOUT) { model.state.tasks?.workspaces?.any { it.name == OVERALL } == true }
        compose.click("capture-workspace")
        waitForTag("capture-workspace-work")
        assertFalse(compose.exists("capture-workspace-$OVERALL"))
        compose.click("capture-workspace-work")
        compose.click("capture-start-task")
        compose.click("task-workspace")
        waitForTag("workspace-work")
        assertFalse(compose.exists("workspace-$OVERALL"))
    }

    private companion object {
        const val TIMEOUT = 15_000L
        const val PINNED = "w-0000b001"
        const val WAITING = "w-0000b002"
        const val LATER = "w-0000b003"
        const val REVIEW = "i-0000a002"
        const val FAILURE = "i-0000a003"
        const val SORT = "i-0000e001"
        const val DELEGATE = "i-0000e002"
        const val FLAKY = "c-0000c001"
        const val OVERALL = "secretary"
    }
}
