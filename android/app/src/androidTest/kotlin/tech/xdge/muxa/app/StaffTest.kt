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
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import tech.xdge.muxa.app.ui.wire
import tech.xdge.muxa.protocol.InboxAction
import tech.xdge.muxa.protocol.ItemKind
import tech.xdge.muxa.protocol.StepBy

/**
 * A workspace's staff and hand-offs (ADR 0051 §7) against the demo at `work` level 12: the staff picker,
 * a request's chain in Work detail, the `handoff` item, and Today's `delegate` row. Level 11 shows none.
 */
@OptIn(ExperimentalTestApi::class)
@RunWith(AndroidJUnit4::class)
class StaffTest {
    @get:Rule
    val compose = createEmptyComposeRule()
    private var scenario: ActivityScenario<MainActivity>? = null
    private val model get() = ApplicationProvider.getApplicationContext<MuxaApplication>().model

    @After
    fun close() {
        scenario?.close()
    }

    private fun launchOnInbox() {
        scenario = ActivityScenario.launch(staffDemoIntent())
        // Today leads the Inbox at this level; its first row is the pinned Work item.
        waitForTag("today-row-$OPEN_WORK")
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

    private fun openWork(id: String) {
        scrollTo("work-row-$id")
        compose.click("work-row-$id")
        compose.waitUntil(TIMEOUT) { model.workDetail == id }
    }

    private fun closeDetail() {
        compose.runOnIdle { model.workDetail = null }
        compose.waitUntilDoesNotExist(hasTestTag("work-detail"), TIMEOUT)
    }

    private fun chain() = model.state.requests.first { it.id == CHAIN }

    /** Step 3's sessions: one per approve that started it. */
    private fun thirdSteps() = model.state.sessions.count { it.name.contains("-$CHAIN-v1-s3-") }

    /** Work has three members: the picker defaults to the lead and names each with its description. Personal has one: no picker. */
    @Test
    fun thePickerShowsOnlyWithMoreThanOneMember() {
        launchOnInbox()
        openWork(WORK_WORK)
        detailScrollTo("ask-staff")
        waitForText("ask-staff", "Let work-research decide")
        compose.click("ask-staff")
        waitForText("ask-staff-work-writer", "Turns findings into a draft")
        compose.click("ask-staff-work-writer")
        detailScrollTo("work-ask")
        compose.click("work-ask")
        compose.waitUntil(TIMEOUT) { model.state.requests.any { it.work == WORK_WORK } }
        val step = model.state.requests.first { it.work == WORK_WORK }.steps.single()
        assertEquals("work-writer", step.worker)
        assertEquals(StepBy.USER, step.by)
        closeDetail()
        openWork(OPEN_WORK)
        detailScrollTo("work-ask")
        assertFalse(compose.exists("ask-staff"))
    }

    /** The Capture sheet offers the picker once the workspace has staff to choose from. */
    @Test
    fun theCaptureSheetAsksANamedMember() {
        launchOnInbox()
        compose.click("sidebar-new-task")
        waitForTag("capture-text")
        compose.onNodeWithTag("capture-text").performTextInput("Check the error budget")
        assertFalse(compose.exists("capture-staff"))
        compose.click("capture-workspace")
        compose.click("capture-workspace-work")
        compose.click("capture-staff")
        compose.click("capture-staff-work-review")
        compose.click("capture-ask")
        compose.waitUntil(TIMEOUT) {
            val made = model.state.work.firstOrNull { it.title == "Check the error budget" }
            made != null && model.state.requests.firstOrNull { it.work == made.id }?.steps?.singleOrNull()?.worker == "work-review"
        }
    }

    /** Work detail shows the chain: each step with its delivery, Result and notes, then the waiting hand-off, greyed. */
    @Test
    fun theDetailShowsTheChainRows() {
        launchOnInbox()
        openWork(CHAIN_WORK)
        detailScrollTo("request-chain-$CHAIN")
        waitForText("request-chain-$CHAIN", "work-research → work-writer")
        waitForText("chain-step-$CHAIN-1", "Step 1 · work-research")
        waitForText("chain-delivery-$CHAIN-1", "taken by the lead")
        detailScrollTo("chain-result-$CHAIN-2")
        waitForText("chain-result-$CHAIN-2", "The table compares price")
        detailScrollTo("chain-notes-$CHAIN-2-apply")
        waitForText("chain-notes-$CHAIN-2", ".muxa/assistant/staff/work-writer/NOTES.md")
        detailScrollTo("chain-handoff-$CHAIN")
        waitForText("chain-handoff-$CHAIN", "Step 3 · work-review")
        waitForText("chain-handoff-state-$CHAIN", "Waits for your approval")
        waitForTag("chain-approve-$CHAIN")
        waitForTag("chain-reject-$CHAIN")
        compose.click("chain-notes-$CHAIN-2-apply")
        waitForTag("chain-notes-$CHAIN-2-applied")
    }

    /** Approve in Work detail starts step 3 once: one run, and the greyed row becomes the step. */
    @Test
    fun approveStartsTheNextStepOnce() {
        launchOnInbox()
        openWork(CHAIN_WORK)
        detailScrollTo("chain-approve-$CHAIN")
        compose.click("chain-approve-$CHAIN")
        compose.waitUntil(TIMEOUT) { chain().steps.size == 3 }
        detailScrollTo("chain-step-$CHAIN-3")
        waitForText("chain-step-$CHAIN-3", "work-review")
        compose.waitUntilDoesNotExist(hasTestTag("chain-handoff-$CHAIN"), TIMEOUT)
        compose.waitUntil(TIMEOUT) { thirdSteps() == 1 }
        assertEquals(StepBy.HANDOFF, chain().steps.last().by)
    }

    /** Reject ends the chain with the Results it has: no step 3, and the row says so. */
    @Test
    fun rejectEndsTheChain() {
        launchOnInbox()
        openWork(CHAIN_WORK)
        detailScrollTo("chain-reject-$CHAIN")
        compose.click("chain-reject-$CHAIN")
        waitForText("chain-handoff-state-$CHAIN", "The chain ended")
        assertFalse(compose.exists("chain-approve-$CHAIN"))
        assertEquals(2, chain().steps.size)
        assertEquals(0, thirdSteps())
    }

    /** The Inbox's `handoff` item names both members; Approve there starts the step too. */
    @Test
    fun theInboxItemNamesBothMembers() {
        launchOnInbox()
        scrollTo("item-row-$HANDOFF")
        compose.click("item-row-$HANDOFF")
        waitForTag("item-handoff")
        waitForText("item-question", "work-writer asks work-review")
        waitForText("item-detail", "work-writer (step 2)")
        waitForText("item-detail", "work-review (step 3)")
        waitForTag("item-${InboxAction.REJECT.wire}")
        compose.click("item-${InboxAction.APPROVE.wire}")
        compose.waitUntil(TIMEOUT) { chain().steps.size == 3 }
        compose.waitUntil(TIMEOUT) { thirdSteps() == 1 }
    }

    /** Today's `delegate` row names the member the plan picked. */
    @Test
    fun todaysDelegateRowNamesTheMember() {
        launchOnInbox()
        scrollTo("today-row-$DELEGATE")
        waitForText("today-staff-$DELEGATE", "Asks home-planner in personal")
    }

    /** A level 11 host keeps today's UI: no picker, no chain, no `handoff` item. */
    @Test
    fun aLevel11HostShowsNone() {
        scenario = ActivityScenario.launch(policyDemoIntent())
        waitForTag("today-row-$OPEN_WORK")
        assertFalse(model.state.inboxItems.any { it.kind == ItemKind.HANDOFF })
        assertFalse(model.state.requests.any { it.steps.isNotEmpty() })
        openWork(WORK_WORK)
        detailScrollTo("work-ask")
        assertFalse(compose.exists("ask-staff"))
        assertFalse(compose.exists("request-chain-$CHAIN"))
        closeDetail()
        assertFalse(compose.exists("today-staff-$DELEGATE"))
    }

    private companion object {
        const val TIMEOUT = 15_000L
        const val OPEN_WORK = "w-0000b001"
        const val WORK_WORK = "w-0000b002"
        const val CHAIN_WORK = "w-0000b005"
        const val CHAIN = "q-0000a002"
        const val HANDOFF = "i-0000f001"
        const val DELEGATE = "i-0000e002"
    }
}
