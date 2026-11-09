package tech.xdge.muxa.demo

import kotlinx.coroutines.asCoroutineDispatcher
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test
import tech.xdge.muxa.protocol.InboxAction
import tech.xdge.muxa.protocol.ItemState
import tech.xdge.muxa.protocol.WORK_ASK_AGENT
import tech.xdge.muxa.protocol.WORK_DELETE_INBOX
import tech.xdge.muxa.session.PaneSession
import tech.xdge.muxa.session.deletable
import tech.xdge.muxa.session.isProtected
import java.util.concurrent.Executors

/** The demo's Protect and Delete Old at `work` level 8 (ADR 0046). */
class DemoInboxDeleteTest {
    private suspend fun connected(level: Int = WORK_DELETE_INBOX, oldItems: Boolean = false): PaneSession {
        val session = PaneSession(Executors.newSingleThreadExecutor().asCoroutineDispatcher())
        session.connect { DemoBridge(workLevel = level, oldItems = oldItems) }
        return session
    }

    private fun PaneSession.item(id: String) = state.value.inboxItems.first { it.id == id }

    @Test
    fun protectWorksOnItemsAndEntriesOpenOrClosed() = runBlocking {
        val session = connected()
        assertTrue(session.state.value.canDeleteInbox)
        assertTrue(session.protect(session.item("i-0000a002"), protected = true).isProtected)
        assertTrue(session.protect(session.item("e-000000a1"), protected = true).isProtected)
        assertTrue(session.protect(session.item("i-0000a004"), protected = true).isProtected)
        // Still answerable.
        assertEquals(ItemState.ANSWERED, session.answer(session.item("i-0000a002"), InboxAction.KEEP_OPEN).state)
        session.loadWork()
        assertTrue(session.item("e-000000a1").isProtected)
        assertFalse(session.protect(session.item("e-000000a1"), protected = false).isProtected)
    }

    @Test
    fun deleteOldRemovesOldUnprotectedItemsAndKeepsProtectedOnes() = runBlocking {
        val session = connected(oldItems = true)
        val old = listOf("i-0000a003", "e-000000a3")
        // The answered item is one day old; at two days only the aged failure and proposal count.
        assertEquals(old, session.state.value.inboxItems.deletable(2).map { it.id })
        session.protect(session.item("e-000000a3"), protected = true)
        assertEquals(1, session.deleteInbox(3))
        val ids = session.state.value.inboxItems.map { it.id }
        assertFalse("i-0000a003" in ids)
        assertTrue("e-000000a3" in ids)
        assertTrue("i-0000a004" in ids)
        assertEquals(0, session.deleteInbox(5))
    }

    @Test
    fun withoutOldItemsNothingIsOlderThanTwoDays() = runBlocking {
        val session = connected()
        assertTrue(session.state.value.inboxItems.deletable(2).isEmpty())
        assertEquals(0, session.deleteInbox(3))
        assertEquals(7, session.state.value.inboxItems.size)
    }

    @Test
    fun aRoutinesEntriesAreDeletedUnlessProtected() {
        val now = 1_000_000L
        val work = DemoWork.tests(now)
        work.protect("e-000000a2", now)
        assertEquals(2, work.deleteBefore(now))
        assertEquals(listOf("e-000000a2"), work.shownEntries.map { it.id })
    }

    @Test
    fun aDayCountOutOfRangeIsRefused() = runBlocking {
        val session = connected()
        refused { session.deleteInbox(0) }
        refused { session.deleteInbox(3651) }
    }

    @Test
    fun aHostBeforeLevelSevenOffersNeither() = runBlocking {
        val session = connected(WORK_ASK_AGENT)
        assertFalse(session.state.value.canDeleteInbox)
        refused { session.deleteInbox(1) }
        refused { session.protect(session.item("i-0000a002"), protected = true) }
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
