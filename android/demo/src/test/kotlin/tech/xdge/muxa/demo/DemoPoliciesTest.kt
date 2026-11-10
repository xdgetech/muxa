package tech.xdge.muxa.demo

import kotlinx.coroutines.asCoroutineDispatcher
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test
import tech.xdge.muxa.protocol.BasisKind
import tech.xdge.muxa.protocol.CorrectionKind
import tech.xdge.muxa.protocol.HypothesisState
import tech.xdge.muxa.protocol.InboxAction
import tech.xdge.muxa.protocol.ItemKind
import tech.xdge.muxa.protocol.WORK_PLANS
import tech.xdge.muxa.protocol.WORK_POLICIES
import tech.xdge.muxa.protocol.WorkAction
import tech.xdge.muxa.session.Cited
import tech.xdge.muxa.session.PaneSession
import tech.xdge.muxa.session.shown
import tech.xdge.muxa.session.todayRows
import java.util.concurrent.Executors

/** The demo's policies, corrections and hypotheses at `work` level 11, as the UI tests use them (ADR 0050). */
class DemoPoliciesTest {
    private suspend fun connected(level: Int = WORK_POLICIES): PaneSession {
        val session = PaneSession(Executors.newSingleThreadExecutor().asCoroutineDispatcher())
        session.connect { DemoBridge(workLevel = level, assistPace = AssistPace.NEVER) }
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

    private val PaneSession.memory get() = state.value.memory

    private fun PaneSession.item(id: String) = state.value.inboxItems.first { it.id == id }

    @Test
    fun levelElevenListsTheMemoryAndThePlanCitesIt() = runBlocking {
        val s = connected().state.value
        assertEquals(listOf("y-0000f001"), s.memory.policies.map { it.id })
        assertEquals(listOf("h-0000f101", "h-0000f102"), s.memory.hypotheses.map { it.id })
        assertEquals(3, s.memory.corrections.size)
        assertTrue(s.inboxItems.any { it.kind == ItemKind.HYPOTHESIS && it.hypothesis?.hypothesis == "h-0000f101" })
        val basis = s.plans.shown()!!.top.first().basis
        assertTrue(s.memory.cited(basis.first { it.kind == BasisKind.POLICY }) is Cited.PolicyRecord)
        assertEquals(Cited.Forgotten("x-0000f2ff"), s.memory.cited(basis.last()))
    }

    /** Try the Demo runs level 11: the showcase's plan cites its policy and its hypothesis asks in the Inbox. */
    @Test
    fun theShowcaseHasPoliciesAndAHypothesisToConfirm() = runBlocking {
        val session = PaneSession(Executors.newSingleThreadExecutor().asCoroutineDispatcher())
        session.connect { DemoBridge(DemoBridge.Content.SHOWCASE, workLevel = WORK_POLICIES) }
        val s = session.state.value
        assertEquals(2, s.memory.policies.size)
        assertTrue(s.inboxItems.any { it.kind == ItemKind.HYPOTHESIS })
        val cites = s.plans.shown()!!.top.flatMap { it.basis }.mapNotNull { s.memory.cited(it) }
        assertTrue(cites.isNotEmpty() && cites.none { it is Cited.Forgotten })
    }

    @Test
    fun levelTenHasNoMemoryAndRefusesThePolicyRequests() = runBlocking {
        val session = connected(WORK_PLANS)
        val s = session.state.value
        assertTrue(s.memory.policies.isEmpty() && s.memory.hypotheses.isEmpty())
        assertTrue(s.inboxItems.none { it.kind == ItemKind.HYPOTHESIS })
        assertTrue(s.plans.shown()!!.top.flatMap { it.basis }.none { it.kind == BasisKind.POLICY })
        refused { session.addPolicy("Recruiting first.", "all") }
        Unit
    }

    @Test
    fun addEditAndDeleteAPolicy() = runBlocking {
        val session = connected()
        val added = session.addPolicy("Recruiting comes first.", "work", until = System.currentTimeMillis() / 1000 + 3_600)
        assertTrue(session.memory.policies.any { it.id == added.id })
        val edited = session.editPolicy(added, text = "Recruiting comes first this week.")
        assertEquals(added.rev + 1, edited.rev)
        assertTrue(refused { session.editPolicy(added, scope = "all") } is PaneSession.Failure.Refused)
        assertTrue(refused { session.addPolicy("x", "secretary") } is PaneSession.Failure.Bridge)
        session.deletePolicy(edited)
        assertTrue(session.memory.policies.none { it.id == added.id })
    }

    @Test
    fun reorderAndNotTodayRecordOneCorrectionEach() = runBlocking {
        val session = connected()
        val plan = session.state.value.plans.shown()!!
        val corrections = session.memory.corrections.size
        val order = plan.top.mapNotNull { it.work ?: it.item }.reversed()
        val reordered = session.reorderPlan(plan, order)
        assertEquals(order, reordered.todayRows.map { it.work ?: it.item })
        val again = session.reorderPlan(reordered, order)
        assertEquals(reordered.rev, again.rev)
        val dismissed = session.dismissRow(again, "w-0000b002", "waiting on legal")
        assertEquals(listOf("w-0000b002"), dismissed.dismissed)
        val newest = session.memory.corrections.first()
        assertEquals(CorrectionKind.DISMISSED, newest.kind)
        assertEquals("waiting on legal", newest.reason)
        assertEquals(corrections + 2, session.memory.corrections.size)
        assertTrue(refused { session.reorderPlan(plan, order) } is PaneSession.Failure.Refused)
    }

    @Test
    fun laterAndPinAreCorrectionsAndLaterHasNoReason() = runBlocking {
        val session = connected()
        val before = session.memory.corrections.size
        session.updateWork(session.state.value.work.first { it.id == "w-0000b002" }, WorkAction.LATER)
        session.updateWork(session.state.value.work.first { it.id == "w-0000b001" }, WorkAction.PIN)
        assertEquals(before + 1, session.memory.corrections.size)
        val deferred = session.memory.corrections.first()
        assertEquals(CorrectionKind.DEFERRED, deferred.kind)
        assertEquals(null, deferred.reason)
    }

    @Test
    fun confirmMakesOnePolicyAndRejectRecordsTheReason() = runBlocking {
        val session = connected()
        val item = session.item("i-0000f101")
        session.answer(item, InboxAction.CONFIRM)
        session.answer(item, InboxAction.CONFIRM)
        assertEquals(1, session.memory.policies.count { it.source == "h-0000f101" })
        assertTrue(session.memory.hypotheses.none { it.id == "h-0000f101" })

        val other = connected()
        other.answer(other.item("i-0000f101"), InboxAction.REJECT, text = "only on Mondays")
        assertEquals(HypothesisState.REJECTED, other.memory.hypotheses.first { it.id == "h-0000f101" }.state)
        assertEquals("only on Mondays", other.memory.corrections.first().reason)
        assertEquals(CorrectionKind.REJECTED, other.memory.corrections.first().kind)
    }

    @Test
    fun forgettingACorrectionRemovesTheHypothesesItGrounds() = runBlocking {
        val session = connected()
        session.forget("x-0000f202")
        assertTrue(session.memory.corrections.none { it.id == "x-0000f202" })
        assertTrue(session.memory.hypotheses.none { it.id == "h-0000f101" })
        assertFalse(session.state.value.inboxItems.any { it.id == "i-0000f101" })
        session.forget("h-0000f102")
        assertTrue(session.memory.hypotheses.isEmpty())
        assertTrue(refused { session.forget("x-00000000") } is PaneSession.Failure.Bridge)
    }

    @Test
    fun aConfirmedHypothesisIsForgottenByDeletingItsPolicy() = runBlocking {
        val session = connected()
        session.answer(session.item("i-0000f101"), InboxAction.CONFIRM)
        val e = refused { session.forget("h-0000f101") }
        assertTrue(PaneSession.describe(e).contains("delete the policy"))
    }
}
