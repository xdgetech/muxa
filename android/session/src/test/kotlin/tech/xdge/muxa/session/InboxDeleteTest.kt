package tech.xdge.muxa.session

import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test
import tech.xdge.muxa.protocol.ErrorBody
import tech.xdge.muxa.protocol.ErrorCode
import tech.xdge.muxa.protocol.InboxAction
import tech.xdge.muxa.protocol.InboxItem
import tech.xdge.muxa.protocol.ItemKind
import tech.xdge.muxa.protocol.ItemState
import tech.xdge.muxa.protocol.Message
import tech.xdge.muxa.protocol.Request

private const val DAY = 86_400L
private const val NOW = 100 * DAY

private fun item(id: String, ageDays: Long, state: ItemState = ItemState.OPEN, protectedAt: Long? = null) = InboxItem(
    id, ItemKind.REVIEW, state, "Question $id", "work", "Title $id", createdAt = NOW - ageDays * DAY - 1,
    protectedAt = protectedAt, answers = if (state == ItemState.OPEN) listOf(InboxAction.KEEP_OPEN, InboxAction.LATER) else emptyList(),
)

/** A host that protects and deletes items as PROTOCOL.md "Inbox items" 4 and 5 say. */
private class DeleteBridge : FakeBridge() {
    var items = listOf(
        item("i-00000001", ageDays = 0),
        item("i-00000002", ageDays = 2),
        item("i-00000003", ageDays = 4, protectedAt = NOW),
        item("i-00000004", ageDays = 6, ItemState.ANSWERED),
    )
    private val _asked = mutableListOf<Request>()
    val asked: List<Request> get() = synchronized(_asked) { _asked.toList() }

    override fun answer(req: Request) {
        when (req) {
            is Request.ListInbox -> {
                record(req)
                emit(Message.InboxList(req.id, emptyList(), items))
                emit(Message.Result(req.id))
            }
            is Request.UpdateInbox -> {
                record(req)
                val saved = items.first { it.id == req.entry }.let {
                    if (req.action == InboxAction.PROTECT) it.copy(protectedAt = NOW) else it.copy(protectedAt = null)
                }
                items = items.map { if (it.id == saved.id) saved else it }
                emit(Message.InboxSaved(req.id, item = saved))
                emit(Message.Result(req.id))
            }
            is Request.DeleteInbox -> {
                record(req)
                if (req.olderThanDays !in 1..3650) {
                    return emit(Message.Result(req.id, ErrorBody(ErrorCode.BAD_REQUEST, "olderThanDays must be 1 to 3650")))
                }
                val gone = items.deletable(req.olderThanDays, NOW)
                items = items - gone.toSet()
                emit(Message.InboxDeleted(req.id, gone.size))
                emit(Message.Result(req.id))
            }
            else -> super.answer(req)
        }
    }

    private fun record(req: Request) = synchronized(_asked) { _asked += req }
}

class InboxDeleteTest {
    private suspend fun connected(bridge: DeleteBridge): PaneSession {
        val session = PaneSession(oneThread())
        session.connect {
            bridge.start()
            bridge
        }
        return session
    }

    private fun bridge(level: Int = 8) = DeleteBridge().apply { capabilities = listOf("task-records", "work:$level") }

    private suspend fun refusal(block: suspend () -> Unit): PaneSession.Failure {
        try {
            block()
        } catch (e: PaneSession.Failure) {
            return e
        }
        fail("expected a refusal")
        throw IllegalStateException()
    }

    @Test
    fun protectSetsTheLockAndUnprotectClearsIt() = runBlocking {
        val bridge = bridge()
        val session = connected(bridge)
        assertTrue(session.state.value.canDeleteInbox)
        val saved = session.protect(session.state.value.inboxItems[0], protected = true)
        assertTrue(saved.isProtected)
        assertTrue(session.state.value.inboxItems[0].isProtected)
        // It is still open and takes its answers.
        assertEquals(ItemState.OPEN, saved.state)
        assertEquals(listOf(InboxAction.KEEP_OPEN, InboxAction.LATER), saved.answers)
        session.protect(session.state.value.inboxItems[0], protected = false)
        assertFalse(session.state.value.inboxItems[0].isProtected)
        assertEquals(
            listOf(InboxAction.PROTECT, InboxAction.UNPROTECT),
            bridge.asked.filterIsInstance<Request.UpdateInbox>().map { it.action },
        )
    }

    @Test
    fun deleteReturnsTheCountAndListsAgain() = runBlocking {
        val bridge = bridge()
        val session = connected(bridge)
        assertEquals(1, session.state.value.inboxItems.deletable(3, NOW).size)
        assertEquals(1, session.deleteInbox(3))
        // The protected item stays, however old.
        assertEquals(listOf("i-00000001", "i-00000002", "i-00000003"), session.state.value.inboxItems.map { it.id })
        assertEquals(1, session.deleteInbox(1))
        assertEquals(listOf("i-00000001", "i-00000003"), session.state.value.inboxItems.map { it.id })
        assertEquals(listOf(3, 1), bridge.asked.filterIsInstance<Request.DeleteInbox>().map { it.olderThanDays })
    }

    @Test
    fun aRefusedDeleteShowsTheHostsReasonAndListsAgain() = runBlocking {
        val bridge = bridge()
        val session = connected(bridge)
        val lists = bridge.asked.count { it is Request.ListInbox }
        val e = refusal { session.deleteInbox(0) }
        assertEquals("olderThanDays must be 1 to 3650", PaneSession.describe(e))
        assertEquals(lists + 1, bridge.asked.count { it is Request.ListInbox })
    }

    @Test
    fun aHostBelowLevelSevenIsNeverAsked() = runBlocking {
        val bridge = bridge(level = 6)
        val session = connected(bridge)
        assertFalse(session.state.value.canDeleteInbox)
        val e = refusal { session.deleteInbox(3) }
        assertEquals(PaneSession.DELETE_INBOX_UNSUPPORTED, PaneSession.describe(e))
        refusal { session.protect(session.state.value.inboxItems[0], protected = true) }
        assertTrue(bridge.asked.none { it is Request.DeleteInbox || it is Request.UpdateInbox })
    }

    @Test
    fun deletableCountsOldUnprotectedItemsOpenOrClosed() {
        val items = bridge().items
        assertEquals(listOf("i-00000004"), items.deletable(5, NOW).map { it.id })
        assertEquals(listOf("i-00000002", "i-00000004"), items.deletable(1, NOW).map { it.id })
        // Exactly one day old is not older than one day.
        assertTrue(listOf(item("i-1", 0).copy(createdAt = NOW - DAY)).deletable(1, NOW).isEmpty())
    }
}
