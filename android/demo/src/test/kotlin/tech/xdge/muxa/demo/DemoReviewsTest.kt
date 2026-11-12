package tech.xdge.muxa.demo

import kotlinx.coroutines.asCoroutineDispatcher
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import tech.xdge.muxa.protocol.Delivery
import tech.xdge.muxa.protocol.InboxAction
import tech.xdge.muxa.protocol.ItemKind
import tech.xdge.muxa.protocol.ItemState
import tech.xdge.muxa.protocol.StepBy
import tech.xdge.muxa.protocol.WORK_RESULT_REVIEW
import tech.xdge.muxa.session.PaneSession
import tech.xdge.muxa.session.assistNote
import java.util.concurrent.Executors

/** The demo's `result` items and Revise at `work` level 16, as the UI tests use them (ADR 0058). */
class DemoReviewsTest {
    private suspend fun connected(set: DemoBridge.Content = DemoBridge.Content.TESTS, pace: AssistPace = AssistPace.NEVER): PaneSession {
        val session = PaneSession(Executors.newSingleThreadExecutor().asCoroutineDispatcher())
        session.connect { DemoBridge(set, workLevel = WORK_RESULT_REVIEW, assistPace = pace) }
        return session
    }

    private fun PaneSession.item(id: String) = state.value.inboxItems.first { it.id == id }

    private fun PaneSession.request(id: String) = state.value.requests.first { it.id == id }

    @Test
    fun theHandoffIsAResultItemAndAResultWithoutStaffHasOneToo() = runBlocking {
        val s = connected().state.value
        assertTrue(s.inboxItems.none { it.kind == ItemKind.HANDOFF })
        val chain = s.inboxItems.first { it.id == CHAIN_ITEM }
        val r = chain.result!!
        assertEquals(ItemKind.RESULT to "work-writer", chain.kind to r.worker)
        assertEquals("work-review", r.handoff?.worker)
        assertEquals(listOf(InboxAction.APPROVE, InboxAction.REPLY, InboxAction.DISMISS, InboxAction.LATER), chain.answers)
        assertTrue(r.next.isNotEmpty() && r.files.isNotEmpty())
        val plain = s.inboxItems.first { it.id == PLAIN_ITEM }
        assertTrue(plain.question.startsWith("The assistant finished: "))
        assertTrue(plain.result!!.findings.isNotEmpty())
        assertEquals(listOf(InboxAction.REPLY, InboxAction.DISMISS, InboxAction.LATER), plain.answers)
        assertEquals(listOf("https://example.com/registrar/pricing"), plain.attachments.links.map { it.url })
        // The write items take Revise.
        assertTrue(s.inboxItems.filter { it.kind == ItemKind.WRITE }.all { InboxAction.REPLY in it.answers })
        // The unread item is the marker: the Work row has none.
        assertNull(assistNote(s.work.first { it.id == "w-0000b003" }, s.requests, s.workLevel))
    }

    @Test
    fun handOffStartsTheProposedStepOnce() = runBlocking {
        val session = connected()
        session.answer(session.item(CHAIN_ITEM), InboxAction.APPROVE)
        val q = session.request(CHAIN)
        assertEquals(listOf("work-research", "work-writer", "work-review"), q.steps.map { it.worker })
        assertEquals(StepBy.HANDOFF, q.steps.last().by)
        assertEquals(InboxAction.APPROVE, session.answer(session.item(CHAIN_ITEM), InboxAction.APPROVE).answer)
        assertEquals(3, session.request(CHAIN).steps.size)
    }

    @Test
    fun sendAsksTheSameMemberAgainAsANewVersion() = runBlocking {
        val session = connected()
        val saved = session.answer(session.item(CHAIN_ITEM), InboxAction.REPLY, text = "Add the prices per seat.")
        assertEquals(ItemState.ANSWERED to InboxAction.REPLY, saved.state to saved.answer)
        val q = session.request(CHAIN)
        assertEquals(2L to "Add the prices per seat.", q.version to q.purpose)
        assertEquals(listOf("work-writer" to StepBy.USER), q.steps.map { it.worker to it.by })
    }

    @Test
    fun aNewResultMakesAResultItemAndDoneClosesIt() = runBlocking {
        val session = connected(pace = AssistPace.AT_ONCE)
        session.answer(session.item(PLAIN_ITEM), InboxAction.REPLY, text = "Check the transfer fee too.")
        session.loadWork()
        assertEquals(Delivery.RESULTED, session.request("q-0000a001").delivery)
        val made = session.state.value.inboxItems.single { it.kind == ItemKind.RESULT && it.result?.request == "q-0000a001" && it.state == ItemState.OPEN }
        assertEquals(3L, made.result?.version)
        assertEquals(InboxAction.DISMISS, session.answer(made, InboxAction.DISMISS).answer)
    }

    @Test
    fun reviseAnswersTheWriteAndAsksItsMemberForANewDraft() = runBlocking {
        val session = connected()
        val write = session.state.value.inboxItems.first { it.kind == ItemKind.WRITE && it.state == ItemState.OPEN }
        val saved = session.answer(write, InboxAction.REPLY, text = "Shorter, and name the release date.")
        assertEquals(InboxAction.REPLY, saved.answer)
        val q = session.request(write.write!!.request)
        assertEquals("Shorter, and name the release date.", q.purpose)
        assertEquals("work-writer", q.steps.single().worker)
        // The other write item stays open.
        assertTrue(session.state.value.inboxItems.any { it.kind == ItemKind.WRITE && it.state == ItemState.OPEN })
    }

    @Test
    fun theShowcaseHasAResultWithAHandoffAndOneWithout() = runBlocking {
        val s = connected(DemoBridge.Content.SHOWCASE).state.value
        val results = s.inboxItems.filter { it.kind == ItemKind.RESULT }
        assertEquals(listOf("i-5c00f001", "i-5c00f002"), results.map { it.id }.sorted())
        assertEquals(1, results.count { it.result?.handoff != null })
    }

    companion object {
        private const val CHAIN = "q-0000a002"
        private const val CHAIN_ITEM = "i-0000f001"
        private const val PLAIN_ITEM = "i-0000f002"
    }
}
