package tech.xdge.muxa.session

import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test
import tech.xdge.muxa.protocol.AssistScope
import tech.xdge.muxa.protocol.AssistantRequest
import tech.xdge.muxa.protocol.Delivery
import tech.xdge.muxa.protocol.ErrorBody
import tech.xdge.muxa.protocol.ErrorCode
import tech.xdge.muxa.protocol.Handoff
import tech.xdge.muxa.protocol.InboxAction
import tech.xdge.muxa.protocol.InboxItem
import tech.xdge.muxa.protocol.ItemKind
import tech.xdge.muxa.protocol.ItemState
import tech.xdge.muxa.protocol.Message
import tech.xdge.muxa.protocol.Request
import tech.xdge.muxa.protocol.RequestStep
import tech.xdge.muxa.protocol.ResultReview
import tech.xdge.muxa.protocol.StepBy
import tech.xdge.muxa.protocol.WorkItem
import tech.xdge.muxa.protocol.WorkState
import tech.xdge.muxa.protocol.WriteProposal

/**
 * A host at [level] with a `result` item that offers a hand-off, and a `write` item. Send and Revise
 * answer as the host does: a new version whose step 1 is the member, asked by the user; [refusal]
 * answers instead, as a budget's `conflict` does.
 */
private class ReviewBridge(level: Int) : FakeBridge() {
    val result = InboxItem(
        "i-00000001", ItemKind.RESULT, ItemState.OPEN, "work-writer asks work-review to: Check the table.", "work", "Choose a tracker", 9,
        summary = "The table compares price and alerts.",
        result = ResultReview(
            "q-00000001", 1, 2, "w-00000001", "work-writer",
            findings = listOf("Both send alerts to chat."), next = listOf("Check the table with the team."),
            files = listOf("/src/comparison.md"), handoff = Handoff("work-review", "Check the table."),
        ),
        answers = listOf(InboxAction.APPROVE, InboxAction.REPLY, InboxAction.DISMISS, InboxAction.LATER),
    )
    val write = InboxItem(
        "i-00000002", ItemKind.WRITE, ItemState.OPEN, "work-writer proposes to write to slack:#release", "work", "Post to #release", 9,
        write = WriteProposal("q-00000001", 1, 2, "work-writer", "Post to #release", "slack:#release", "Release 2.4 is out.", "sha256:00"),
        answers = listOf(InboxAction.APPROVE, InboxAction.REJECT, InboxAction.REPLY, InboxAction.LATER),
    )
    var refusal: ErrorBody? = null
    private val work = WorkItem("w-00000001", 3, "Choose a tracker", WorkState.OPEN, 1, 1, "work", requests = listOf("q-00000001"))

    init {
        capabilities = listOf("task-records", "work:$level")
    }

    override fun answer(req: Request) {
        when (req) {
            is Request.ListInbox -> {
                emit(Message.InboxList(req.id, emptyList(), items = listOf(result, write), work = listOf(work)))
                emit(Message.Result(req.id))
            }
            is Request.UpdateInbox -> {
                refusal?.let { return emit(Message.Result(req.id, it)) }
                val item = if (req.entry == result.id) result else write
                if (req.action == InboxAction.REPLY || req.action == InboxAction.APPROVE) {
                    val worker = if (req.action == InboxAction.APPROVE) "work-review" else "work-writer"
                    val step = RequestStep(1, worker, StepBy.USER, Delivery.PENDING, 10)
                    val q = AssistantRequest(
                        "q-00000001", work.id, 2, "work", AssistScope.RESEARCH, Delivery.PENDING, 10, 1, 10,
                        purpose = req.text, steps = listOf(step),
                    )
                    emit(Message.WorkSaved(req.id, work.copy(rev = 4), q))
                }
                emit(Message.InboxSaved(req.id, item = item.copy(state = ItemState.ANSWERED, answer = req.action, answers = emptyList())))
                emit(Message.Result(req.id))
            }
            else -> super.answer(req)
        }
    }
}

class ReviewTest {
    private suspend fun connected(bridge: ReviewBridge): PaneSession {
        val session = PaneSession(oneThread())
        session.connect {
            bridge.start()
            bridge
        }
        session.loadWork()
        return session
    }

    private fun ReviewBridge.answers() = received.filterIsInstance<Request.UpdateInbox>()

    @Test
    fun sendOnAResultIsAReplyWithTheText() = runBlocking {
        val bridge = ReviewBridge(16)
        val session = connected(bridge)
        assertTrue(session.state.value.reviewsResults)
        val saved = session.answer(bridge.result, InboxAction.REPLY, text = "Also check the rate limits.")
        val sent = bridge.answers().single()
        assertEquals(Triple("i-00000001", InboxAction.REPLY, "Also check the rate limits."), Triple(sent.entry, sent.action, sent.text))
        assertNull(sent.hash)
        assertEquals(ItemState.ANSWERED to InboxAction.REPLY, saved.state to saved.answer)
    }

    @Test
    fun handOffIsApproveWithNoHash() = runBlocking {
        val bridge = ReviewBridge(16)
        val session = connected(bridge)
        val saved = session.answer(bridge.result, InboxAction.APPROVE)
        val sent = bridge.answers().single()
        assertEquals(InboxAction.APPROVE to null, sent.action to sent.hash)
        assertEquals(InboxAction.APPROVE, saved.answer)
    }

    @Test
    fun reviseOnAWriteIsAReplyWithTheText() = runBlocking {
        val bridge = ReviewBridge(16)
        val session = connected(bridge)
        val saved = session.answer(bridge.write, InboxAction.REPLY, text = "Shorter, and name the date.")
        assertEquals(InboxAction.REPLY to "Shorter, and name the date.", bridge.answers().single().let { it.action to it.text })
        assertEquals(InboxAction.REPLY, saved.answer)
    }

    @Test
    fun aBudgetReachedIsTheHostsReasonAndTheItemStaysOpen() = runBlocking {
        val bridge = ReviewBridge(16)
        val session = connected(bridge)
        bridge.refusal = ErrorBody(ErrorCode.CONFLICT, "workspace \"work\" has 1 open assistant runs, its max_open_runs")
        try {
            session.answer(bridge.result, InboxAction.REPLY, text = "Go on.")
            fail("a budget reached refuses Send")
        } catch (e: PaneSession.Failure.Refused) {
            assertTrue(PaneSession.describe(e).contains("max_open_runs"))
        }
        assertEquals(ItemState.OPEN, session.state.value.inboxItems.first { it.id == bridge.result.id }.state)
    }

    @Test
    fun theItemScreenNamesTheMemberAndTheAnswers() {
        val bridge = ReviewBridge(16)
        assertEquals("Hand off to work-review", bridge.result.handOffTitle)
        assertEquals("Tell work-writer what to do next…", bridge.result.reviewPlaceholder)
        assertEquals("Ask work-writer for a new draft…", bridge.write.reviewPlaceholder)
        // Without a staff member and a hand-off: the assistant, and no Hand off.
        val plain = bridge.result.copy(result = bridge.result.result!!.copy(worker = null, handoff = null))
        assertNull(plain.handOffTitle)
        assertEquals("Tell the assistant what to do next…", plain.reviewPlaceholder)
        // Hand off is a decision, Send the reply box, Done the ✓.
        val buttons = bridge.result.buttons
        assertEquals(listOf(InboxAction.APPROVE), buttons.decisions)
        assertEquals(InboxAction.REPLY to InboxAction.DISMISS, buttons.text to buttons.done)
        assertEquals(listOf(InboxAction.APPROVE, InboxAction.REJECT), bridge.write.buttons.decisions)
        assertEquals(InboxAction.REPLY, bridge.write.buttons.text)
        // Other kinds keep the agent's words.
        assertNull(bridge.result.copy(kind = ItemKind.REVIEW).reviewPlaceholder)
    }

    @Test
    fun resultReadyShowsOnlyBelowLevel16() {
        val q = AssistantRequest("q-00000001", "w-00000001", 1, "work", AssistScope.RESEARCH, Delivery.RESULTED, 1, 1, 1)
        val item = WorkItem("w-00000001", 1, "t", WorkState.OPEN, 1, 1, "work", requests = listOf(q.id))
        assertEquals(RESULT_READY, assistNote(item, listOf(q), workLevel = 15))
        assertNull(assistNote(item, listOf(q), workLevel = 16))
        // Other states keep their note at every level.
        assertEquals("assistant working", assistNote(item, listOf(q.copy(delivery = Delivery.PROCESSING)), workLevel = 16))
    }

    @Test
    fun aLevel15HostDoesNotReviewResults() = runBlocking {
        val session = connected(ReviewBridge(15))
        assertFalse(session.state.value.reviewsResults)
    }
}
