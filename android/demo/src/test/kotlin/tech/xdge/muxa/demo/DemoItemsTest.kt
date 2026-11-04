package tech.xdge.muxa.demo

import kotlinx.coroutines.asCoroutineDispatcher
import kotlinx.coroutines.delay
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test
import tech.xdge.muxa.protocol.InboxAction
import tech.xdge.muxa.protocol.ItemState
import tech.xdge.muxa.protocol.TaskStatus
import tech.xdge.muxa.protocol.WORK_ASK
import tech.xdge.muxa.protocol.WORK_ITEMS
import tech.xdge.muxa.session.PaneSession
import tech.xdge.muxa.session.waiting
import java.util.concurrent.Executors

/** The demo's Inbox items at `work` level 3, as the UI tests use them (ADR 0033). */
class DemoItemsTest {
    private suspend fun connected(level: Int = WORK_ITEMS): PaneSession {
        val session = PaneSession(Executors.newSingleThreadExecutor().asCoroutineDispatcher())
        session.connect { DemoBridge(workLevel = level) }
        return session
    }

    private fun PaneSession.item(id: String) = state.value.inboxItems.first { it.id == id }

    @Test
    fun theAskComesFirstThenNewestAndTheEntriesAreItems() = runBlocking {
        val s = connected().state.value
        assertEquals(
            listOf("i-0000a001", "i-0000a003", "i-0000a002", "e-000000a1", "e-000000a2", "e-000000a3"),
            s.inboxItems.waiting().map { it.id },
        )
    }

    @Test
    fun anOlderLevelServesNoItems() = runBlocking {
        assertTrue(connected(WORK_ASK).state.value.inboxItems.isEmpty())
    }

    @Test
    fun acceptMarksTheTaskDone() = runBlocking {
        val session = connected()
        val saved = session.answer(session.item("i-0000a002"), InboxAction.ACCEPT)
        assertEquals(ItemState.ANSWERED, saved.state)
        assertEquals(TaskStatus.DONE, session.state.value.tasks?.record("t-login1")?.status)
    }

    @Test
    fun redoNeedsATextAndAnAnswerNotOfferedIsRefused() = runBlocking {
        val session = connected()
        val review = session.item("i-0000a002")
        refused { session.answer(review, InboxAction.REDO) }
        refused { session.answer(review, InboxAction.RETRY) }
        assertEquals(ItemState.ANSWERED, session.answer(review, InboxAction.REDO, text = "Keep the old page too.").state)
    }

    /** ADR 0033 §4a: every Task item offers `reply`; it needs a text, which the agent gets as its next prompt. */
    @Test
    fun aReplyNeedsATextAndReachesTheAgent() = runBlocking {
        val session = connected()
        for (id in listOf("i-0000a001", "i-0000a002", "i-0000a003")) assertTrue(InboxAction.REPLY in session.item(id).answers)
        val ask = session.item("i-0000a001")
        refused { session.answer(ask, InboxAction.REPLY) }
        session.watch("%1")
        session.follow("%1")
        waitUntil { session.state.value.chat?.loaded == true }
        assertEquals(InboxAction.REPLY, session.answer(ask, InboxAction.REPLY, text = "Keep build; run make clean.").answer)
        waitUntil { session.state.value.chat?.entries?.lastOrNull()?.text == "Keep build; run make clean." }
        assertEquals(TaskStatus.OPEN, session.state.value.tasks?.record("t-auth01")?.status)
    }

    /** The showcase's review names its doc and its PR in the summary, for the Markdown links. */
    @Test
    fun theShowcaseReviewLinksItsEvidence() = runBlocking {
        val session = PaneSession(Executors.newSingleThreadExecutor().asCoroutineDispatcher())
        session.connect { DemoBridge(DemoBridge.Content.SHOWCASE) }
        val review = session.item("i-5c000002")
        assertTrue(review.summary.orEmpty().contains("`README.md`"))
        assertTrue(review.summary.orEmpty().contains("(https://github.com/example/dashboard/pull/318)"))
        assertEquals(listOf("README.md"), review.attachments.docs)
        assertTrue(InboxAction.REPLY in review.answers)
    }

    private suspend fun waitUntil(condition: () -> Boolean) {
        val deadline = System.currentTimeMillis() + 5_000
        while (!condition()) {
            if (System.currentTimeMillis() > deadline) throw AssertionError("condition not met in 5 s")
            delay(10)
        }
    }

    @Test
    fun rejectingAProposalDismissesItsEntry() = runBlocking {
        val session = connected()
        val saved = session.answer(session.item("e-000000a3"), InboxAction.REJECT)
        assertEquals(InboxAction.DISMISS, saved.answer)
        assertEquals(ItemState.ANSWERED, session.item("e-000000a3").state)
    }

    @Test
    fun readAndLaterKeepTheItem() = runBlocking {
        val session = connected()
        session.markRead(session.item("i-0000a001"))
        assertTrue(session.item("i-0000a001").readAt != null)
        val until = System.currentTimeMillis() / 1000 + 3_600
        assertEquals(ItemState.LATER, session.answer(session.item("i-0000a001"), InboxAction.LATER, until = until).state)
    }

    private suspend fun refused(block: suspend () -> Unit) {
        try {
            block()
        } catch (e: PaneSession.Failure) {
            return
        }
        fail("expected a refusal")
    }
}
