package tech.xdge.muxa.app

import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.hasContentDescription
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
import tech.xdge.muxa.protocol.ItemState
import tech.xdge.muxa.protocol.StepBy

/**
 * A request's Result reviewed like a Task (ADR 0058 §7) against the demo at `work` level 16: the `result`
 * item's sections, Hand off, Send, and Revise on a `write` item.
 */
@OptIn(ExperimentalTestApi::class)
@RunWith(AndroidJUnit4::class)
class ReviewTest {
    @get:Rule
    val compose = createEmptyComposeRule()
    private var scenario: ActivityScenario<MainActivity>? = null
    private val model get() = ApplicationProvider.getApplicationContext<MuxaApplication>().model

    @After
    fun close() {
        scenario?.close()
    }

    private fun launchOnInbox() {
        scenario = ActivityScenario.launch(reviewDemoIntent())
        waitForTag("today-row-$OPEN_WORK")
    }

    private fun waitForTag(tag: String) = compose.waitUntilAtLeastOneExists(hasTestTag(tag), TIMEOUT)

    private fun waitForText(tag: String, text: String) = compose.waitForTextIn(tag, text, TIMEOUT)

    private fun openItem(id: String) {
        compose.onNodeWithTag("inbox-list").performScrollToNode(hasTestTag("item-row-$id"))
        compose.click("item-row-$id")
        waitForTag("item-detail")
    }

    private fun item(id: String) = model.state.inboxItems.first { it.id == id }

    private fun send(text: String) {
        compose.onNodeWithTag("item-reply").performTextInput(text)
        compose.click("item-send")
    }

    /** The summary, then Findings, Next Steps and Files; the reply box asks the assistant, and ✓ is Done. */
    @Test
    fun aResultItemShowsFindingsAndNextSteps() {
        launchOnInbox()
        openItem(PLAIN)
        waitForText("item-question", "The assistant finished")
        waitForText("item-detail", "Stay with the current registrar")
        waitForText("item-result-findings", "Findings")
        waitForText("item-result-findings", "The current registrar raised its price")
        waitForText("item-result-next", "Next Steps")
        waitForText("item-result-next", "Turn on auto-renew.")
        waitForText("item-result-files", "registrars.md")
        waitForText("item-reply", "Tell the assistant what to do next…")
        compose.waitUntilAtLeastOneExists(hasTestTag("item-${InboxAction.DISMISS.wire}") and hasContentDescription("Done"), TIMEOUT)
        // No hand-off is offered.
        assertFalse(compose.exists("item-${InboxAction.APPROVE.wire}"))
    }

    /** Hand off names the next member and starts its step once; the item is answered. */
    @Test
    fun handOffAnswersTheItem() {
        launchOnInbox()
        openItem(CHAIN_ITEM)
        waitForText("item-${InboxAction.APPROVE.wire}", "Hand off to work-review")
        waitForText("item-reply", "Tell work-writer what to do next…")
        compose.click("item-${InboxAction.APPROVE.wire}")
        compose.waitUntil(TIMEOUT) { item(CHAIN_ITEM).answer == InboxAction.APPROVE }
        val chain = model.state.requests.first { it.id == CHAIN }
        assertEquals(listOf("work-research", "work-writer", "work-review"), chain.steps.map { it.worker })
    }

    /** Send asks the member again with the text: a new version, and the item is answered. */
    @Test
    fun sendAnswersTheItem() {
        launchOnInbox()
        openItem(CHAIN_ITEM)
        send("Add the price per seat.")
        compose.waitUntil(TIMEOUT) { item(CHAIN_ITEM).answer == InboxAction.REPLY }
        compose.waitUntil(TIMEOUT) { model.state.requests.first { it.id == CHAIN }.version == 2L }
        val q = model.state.requests.first { it.id == CHAIN }
        assertEquals("Add the price per seat." to StepBy.USER, q.purpose to q.steps.first().by)
        assertEquals("work-writer", q.steps.first().worker)
    }

    /** Revise on a `write` item asks its member for a new draft and closes the item. */
    @Test
    fun reviseClosesTheWriteItem() {
        launchOnInbox()
        openItem(POST)
        waitForTag("item-write")
        waitForText("item-reply", "Ask work-writer for a new draft…")
        send("Shorter, and name the release date.")
        compose.waitUntil(TIMEOUT) { item(POST).state == ItemState.ANSWERED }
        assertEquals(InboxAction.REPLY, item(POST).answer)
        assertEquals(ItemKind.WRITE, item(POST).kind)
    }

    /** Work detail offers the same hand-off: Hand off and Done answer the `result` item; Done ends the chain. */
    @Test
    fun workDetailEndsTheChainWithDone() {
        launchOnInbox()
        compose.onNodeWithTag("inbox-list").performScrollToNode(hasTestTag("work-row-$CHAIN_WORK"))
        compose.click("work-row-$CHAIN_WORK")
        compose.waitUntil(TIMEOUT) { model.workDetail == CHAIN_WORK }
        compose.onNodeWithTag("work-detail").performScrollToNode(hasTestTag("chain-reject-$CHAIN"))
        waitForText("chain-approve-$CHAIN", "Hand off to work-review")
        waitForText("chain-reject-$CHAIN", "Done")
        compose.click("chain-reject-$CHAIN")
        waitForText("chain-handoff-state-$CHAIN", "The chain ended")
        assertEquals(InboxAction.DISMISS, item(CHAIN_ITEM).answer)
        assertEquals(2, model.state.requests.first { it.id == CHAIN }.steps.size)
    }

    private companion object {
        const val TIMEOUT = 15_000L
        const val OPEN_WORK = "w-0000b001"
        const val CHAIN = "q-0000a002"
        const val CHAIN_WORK = "w-0000b005"
        const val CHAIN_ITEM = "i-0000f001"
        const val PLAIN = "i-0000f002"
        const val POST = "i-0000d101"
    }
}
