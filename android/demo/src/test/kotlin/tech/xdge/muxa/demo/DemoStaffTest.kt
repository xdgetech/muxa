package tech.xdge.muxa.demo

import kotlinx.coroutines.asCoroutineDispatcher
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test
import tech.xdge.muxa.protocol.AssistScope
import tech.xdge.muxa.protocol.Delivery
import tech.xdge.muxa.protocol.InboxAction
import tech.xdge.muxa.protocol.ItemKind
import tech.xdge.muxa.protocol.ItemState
import tech.xdge.muxa.protocol.StepBy
import tech.xdge.muxa.protocol.WORK_POLICIES
import tech.xdge.muxa.protocol.WORK_STAFF
import tech.xdge.muxa.session.PaneSession
import tech.xdge.muxa.session.shown
import tech.xdge.muxa.session.staffOf
import java.util.concurrent.Executors

/** The demo's staff and hand-offs at `work` level 12, as the UI tests use them (ADR 0051). */
class DemoStaffTest {
    private suspend fun connected(level: Int = WORK_STAFF, pace: AssistPace = AssistPace.NEVER): PaneSession {
        val session = PaneSession(Executors.newSingleThreadExecutor().asCoroutineDispatcher())
        session.connect { DemoBridge(workLevel = level, assistPace = pace) }
        return session
    }

    private suspend fun refused(block: suspend () -> Unit): PaneSession.Failure {
        try {
            block()
        } catch (e: PaneSession.Failure) {
            return e
        }
        fail("expected a refusal")
        throw IllegalStateException()
    }

    private fun PaneSession.request(id: String) = state.value.requests.first { it.id == id }

    private fun PaneSession.item(id: String) = state.value.inboxItems.first { it.id == id }

    private fun PaneSession.work(id: String) = state.value.work.first { it.id == id }

    /** The sessions of step 3's runs: one per approve that started it. */
    private fun PaneSession.thirdSteps() = state.value.sessions.count { it.name.contains("-$CHAIN-v1-s3-") }

    @Test
    fun levelTwelveListsTheStaffTheChainAndItsHandoff() = runBlocking {
        val s = connected().state.value
        assertEquals(listOf("work-research", "work-writer", "work-review"), s.assistant.staffOf("work").map { it.worker })
        assertEquals(listOf("home-planner"), s.assistant.staffOf("personal").map { it.worker })
        val q = s.requests.first { it.id == CHAIN }
        assertEquals(listOf("work-research", "work-writer"), q.steps.map { it.worker })
        assertEquals(listOf(2, 1), q.results.map { it.step })
        val handoff = s.inboxItems.first { it.id == HANDOFF }
        assertEquals(ItemKind.HANDOFF, handoff.kind)
        assertEquals("work-review", handoff.handoff?.worker)
        assertEquals("home-planner", s.inboxItems.first { it.id == "i-0000e002" }.delegate?.worker)
        assertEquals("i-0000e002", s.plans.shown()!!.top.last().item)
    }

    @Test
    fun levelElevenHasNoStaffAndRefusesANamedMember() = runBlocking {
        val session = connected(WORK_POLICIES)
        val s = session.state.value
        assertTrue(s.assistant.staffOf("work").isEmpty())
        assertTrue(s.requests.none { it.id == CHAIN } && s.requests.all { it.steps.isEmpty() })
        assertTrue(s.inboxItems.none { it.kind == ItemKind.HANDOFF })
        assertNull(s.inboxItems.first { it.id == "i-0000e002" }.delegate?.worker)
        // The client refuses before anything is sent.
        refused { session.requestAssistant(session.work(OPEN_WORK), AssistScope.RESEARCH, workspace = "work", worker = "work-writer") }
        Unit
    }

    @Test
    fun aNamedMemberRunsStepOneAndNobodyNamedIsTheLead() = runBlocking {
        val session = connected()
        val asked = session.requestAssistant(session.work(OPEN_WORK), AssistScope.RESEARCH, workspace = "work", worker = "work-writer")
        assertEquals("work-writer", asked.steps.single().worker)
        assertEquals(StepBy.USER, asked.steps.single().by)
        val unknown = refused {
            session.requestAssistant(session.work("w-0000b002"), AssistScope.RESEARCH, worker = "nobody")
        }
        assertTrue(PaneSession.describe(unknown).contains("work-research, work-writer, work-review"))
        session.cancelRequest(asked)
        val lead = session.requestAssistant(session.work(OPEN_WORK), AssistScope.RESEARCH)
        assertEquals("home-planner", lead.steps.single().worker)
        assertEquals(StepBy.LEAD, lead.steps.single().by)
    }

    @Test
    fun approveStartsTheNextStepOnce() = runBlocking {
        val session = connected()
        session.answer(session.item(HANDOFF), InboxAction.APPROVE)
        val q = session.request(CHAIN)
        assertEquals(listOf(1, 2, 3), q.steps.map { it.step })
        assertEquals(StepBy.HANDOFF, q.steps.last().by)
        assertEquals(Delivery.PENDING, q.delivery)
        assertEquals(1, session.thirdSteps())
        val again = session.answer(session.item(HANDOFF), InboxAction.APPROVE)
        assertEquals(InboxAction.APPROVE, again.answer)
        session.refresh()
        assertEquals(3, session.request(CHAIN).steps.size)
        assertEquals(1, session.thirdSteps())
    }

    @Test
    fun rejectEndsTheChainWithTheResultsItHas() = runBlocking {
        val session = connected()
        session.answer(session.item(HANDOFF), InboxAction.REJECT)
        assertEquals(ItemState.ANSWERED, session.item(HANDOFF).state)
        val q = session.request(CHAIN)
        assertEquals(2, q.steps.size)
        assertEquals(2, q.results.size)
        assertEquals(0, session.thirdSteps())
    }

    @Test
    fun theNextStepsResultNamesItsMemberAndKeepsTheEarlierOnes() = runBlocking {
        val session = connected(pace = AssistPace.AT_ONCE)
        session.answer(session.item(HANDOFF), InboxAction.APPROVE)
        session.loadWork()
        val q = session.request(CHAIN)
        assertEquals(Delivery.RESULTED, q.steps.last().delivery)
        assertEquals(listOf(3, 2, 1), q.results.map { it.step })
        assertEquals("work-review", q.results.first().worker)
        assertTrue(q.results.none { it.stale })
    }

    @Test
    fun aNewVersionEndsTheWaitingHandoff() = runBlocking {
        val session = connected()
        val asked = session.requestAssistant(session.work(CHAIN_WORK), AssistScope.RESEARCH)
        assertEquals(2L, asked.version)
        assertEquals(1, asked.steps.size)
        assertEquals(ItemState.RESOLVED, session.item(HANDOFF).state)
    }

    @Test
    fun aMembersNotesGoToItsOwnFile() = runBlocking {
        val session = connected()
        val q = session.request(CHAIN)
        val notes = q.results.first { it.step == 2 }.notes
        assertNotNull(notes)
        session.applyNotes(CHAIN, 1, notes!!, step = 2)
        val work = session.state.value.assistant?.delegates?.first { it.workspace == "work" }
        assertNotNull(work?.staff?.first { it.worker == "work-writer" }?.notesAt)
        assertNull(work?.notesAt)
        // Step 1 proposed no notes.
        refused { session.applyNotes(CHAIN, 1, notes, step = 1) }
        Unit
    }

    @Test
    fun theShowcaseHasAChainWaitingOnAHandoff() = runBlocking {
        val session = PaneSession(Executors.newSingleThreadExecutor().asCoroutineDispatcher())
        session.connect { DemoBridge(DemoBridge.Content.SHOWCASE, workLevel = WORK_STAFF) }
        val s = session.state.value
        assertEquals(3, s.assistant.staffOf("work").size)
        assertEquals(2, s.requests.first { it.id == "q-5c00a002" }.steps.size)
        assertTrue(s.inboxItems.any { it.kind == ItemKind.HANDOFF && it.state == ItemState.OPEN })
        assertEquals("work-research", s.inboxItems.first { it.id == "i-5c00e002" }.delegate?.worker)
    }

    companion object {
        private const val CHAIN = "q-0000a002"
        private const val CHAIN_WORK = "w-0000b005"
        private const val HANDOFF = "i-0000f001"
        private const val OPEN_WORK = "w-0000b001"
    }
}
