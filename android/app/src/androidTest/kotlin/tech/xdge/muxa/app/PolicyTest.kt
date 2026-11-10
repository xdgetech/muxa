package tech.xdge.muxa.app

import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.junit4.createEmptyComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performScrollToNode
import androidx.compose.ui.test.performTextClearance
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
import tech.xdge.muxa.protocol.CorrectionKind
import tech.xdge.muxa.protocol.HypothesisState
import tech.xdge.muxa.protocol.InboxAction
import tech.xdge.muxa.session.shown

/**
 * Policies, Today's reorder and Not today, Why with Forget, and `hypothesis` items (ADR 0050 §7),
 * against the demo at `work` level 11.
 */
@OptIn(ExperimentalTestApi::class)
@RunWith(AndroidJUnit4::class)
class PolicyTest {
    @get:Rule
    val compose = createEmptyComposeRule()
    private var scenario: ActivityScenario<MainActivity>? = null
    private val model get() = ApplicationProvider.getApplicationContext<MuxaApplication>().model
    private val memory get() = model.state.memory

    @After
    fun close() {
        scenario?.close()
    }

    private fun launchOnInbox() {
        scenario = ActivityScenario.launch(policyDemoIntent())
        waitForTag("today-row-$PINNED")
    }

    private fun waitForTag(tag: String) = compose.waitUntilAtLeastOneExists(hasTestTag(tag), TIMEOUT)

    private fun waitForText(tag: String, text: String) = compose.waitForTextIn(tag, text, TIMEOUT)

    private fun scrollTo(list: String, tag: String) {
        compose.onNodeWithTag(list).performScrollToNode(hasTestTag(tag))
        waitForTag(tag)
    }

    private fun openPolicies() {
        compose.click("today-policies")
        waitForTag("policies")
    }

    @Test
    fun aPolicyIsAddedForThisWeekEditedAndDeleted() {
        launchOnInbox()
        openPolicies()
        waitForText("policy-$POLICY", "This week, the offsite comes before sales calls.")
        compose.click("policies-add")
        compose.onNodeWithTag("policy-text").performTextInput("Recruiting comes first.")
        compose.click("policy-scope")
        assertFalse(compose.exists("policy-scope-$OVERALL"))
        compose.click("policy-scope-work")
        compose.click("policy-end-week")
        compose.click("policy-save")
        compose.waitUntil(TIMEOUT) { memory.policies.any { it.text == "Recruiting comes first." } }
        val added = memory.policies.first { it.text == "Recruiting comes first." }
        assertEquals("work", added.scope)
        assertTrue(added.until != null)

        scrollTo("policies-list", "policy-edit-${added.id}")
        compose.click("policy-edit-${added.id}")
        compose.onNodeWithTag("policy-text").performTextClearance()
        compose.onNodeWithTag("policy-text").performTextInput("Recruiting comes first, always.")
        compose.click("policy-save")
        waitForText("policy-${added.id}", "Recruiting comes first, always.")
        assertEquals(added.rev + 1, memory.policies.first { it.id == added.id }.rev)

        compose.click("policy-delete-${added.id}")
        compose.waitUntil(TIMEOUT) { memory.policies.none { it.id == added.id } }
    }

    /** Confirm makes a policy of the hypothesis; Forget on a correction removes it. */
    @Test
    fun aHypothesisIsConfirmedAndACorrectionForgotten() {
        launchOnInbox()
        openPolicies()
        scrollTo("policies-list", "hypothesis-confirm-$HYPOTHESIS")
        waitForText("hypothesis-$HYPOTHESIS", "based on 2 corrections")
        compose.click("hypothesis-confirm-$HYPOTHESIS")
        compose.waitUntil(TIMEOUT) { memory.policies.any { it.source == HYPOTHESIS } }
        scrollTo("policies-list", "correction-forget-$DEFERRED")
        compose.click("correction-forget-$DEFERRED")
        waitForText("forget-message", "Plans made before keep their reasons.")
        compose.click("forget-confirm")
        compose.waitUntil(TIMEOUT) { memory.corrections.none { it.id == DEFERRED } }
    }

    /** Move Down sets the user's order; Not Today keeps the reason and collapses the row. */
    @Test
    fun todayRowsMoveAndOneIsNotToday() {
        launchOnInbox()
        compose.click("today-more-$PINNED")
        compose.click("today-down-$PINNED")
        compose.waitUntil(TIMEOUT) { model.state.plans.shown()?.userOrder?.take(2) == listOf(WAITING, PINNED) }
        assertEquals(CorrectionKind.REORDERED, memory.corrections.first().kind)

        compose.click("today-more-$REVIEW")
        compose.click("today-not-today-$REVIEW")
        compose.onNodeWithTag("not-today-reason").performTextInput("after lunch")
        compose.click("not-today-confirm")
        waitForTag("today-dismissed-$REVIEW")
        assertFalse(compose.exists("today-row-$REVIEW"))
        assertEquals("after lunch", memory.corrections.first().reason)
    }

    /** Why shows each cited record by its words, "forgotten" for a missing one, and Forget removes one. */
    @Test
    fun whyShowsTheRecordsCitedAndForgetsOne() {
        launchOnInbox()
        compose.click("today-why-$PINNED")
        waitForText("today-cite-$POLICY", "Your policy: This week, the offsite comes before sales calls.")
        waitForText("today-cite-$HYPOTHESIS", "Reviews come before new work in the morning.")
        waitForText("today-cite-$GONE", "forgotten")
        compose.click("today-forget-$PINNED_CORRECTION")
        compose.click("forget-confirm")
        compose.waitUntil(TIMEOUT) { memory.corrections.none { it.id == PINNED_CORRECTION } }
        waitForText("today-cite-$PINNED_CORRECTION", "forgotten")
    }

    /** A `hypothesis` item shows its evidence by count; Reject asks for an optional reason. */
    @Test
    fun aHypothesisItemIsRejectedWithAReason() {
        launchOnInbox()
        scrollTo("inbox-list", "item-row-$HYPOTHESIS_ITEM")
        compose.click("item-row-$HYPOTHESIS_ITEM")
        waitForTag("item-hypothesis")
        waitForText("item-hypothesis-evidence", "Based on 2 corrections")
        waitForTag("item-${InboxAction.CONFIRM.wire}")
        compose.click("item-${InboxAction.REJECT.wire}")
        compose.onNodeWithTag("reject-reason").performTextInput("only on Mondays")
        compose.click("reject-confirm")
        compose.waitUntil(TIMEOUT) { memory.hypotheses.firstOrNull { it.id == HYPOTHESIS }?.state == HypothesisState.REJECTED }
        assertEquals("only on Mondays", memory.corrections.first().reason)
    }

    /** A level 10 host keeps today's UI: no Policies, no row menu, no reason on Reject. */
    @Test
    fun levelTenShowsNoPolicies() {
        scenario = ActivityScenario.launch(planDemoIntent())
        waitForTag("today-row-$PINNED")
        assertFalse(compose.exists("today-policies"))
        assertFalse(compose.exists("today-more-$PINNED"))
        assertTrue(model.state.memory.policies.isEmpty())
    }

    private companion object {
        const val TIMEOUT = 15_000L
        const val PINNED = "w-0000b001"
        const val WAITING = "w-0000b002"
        const val REVIEW = "i-0000a002"
        const val OVERALL = "secretary"
        const val POLICY = "y-0000f001"
        const val HYPOTHESIS = "h-0000f101"
        const val HYPOTHESIS_ITEM = "i-0000f101"
        const val PINNED_CORRECTION = "x-0000f201"
        const val DEFERRED = "x-0000f203"
        const val GONE = "x-0000f2ff"
    }
}
