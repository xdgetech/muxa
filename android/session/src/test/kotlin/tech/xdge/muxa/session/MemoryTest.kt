package tech.xdge.muxa.session

import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test
import tech.xdge.muxa.protocol.ErrorBody
import tech.xdge.muxa.protocol.ErrorCode
import tech.xdge.muxa.protocol.InboxAction
import tech.xdge.muxa.protocol.InboxItem
import tech.xdge.muxa.protocol.ItemKind
import tech.xdge.muxa.protocol.ItemState
import tech.xdge.muxa.protocol.MemoryMode
import tech.xdge.muxa.protocol.MemoryProposal
import tech.xdge.muxa.protocol.Message
import tech.xdge.muxa.protocol.Request
import tech.xdge.muxa.protocol.WorkspaceInfo

/**
 * A host at [level] with one `memory` item. Approve answers as the host does: `bad-request` without
 * the proposal's hash; [refusal] answers it otherwise, as a full WORKSPACE.md's `conflict` does.
 */
private class MemoryBridge(level: Int) : FakeBridge() {
    val memory = MemoryProposal("t-cache1", "Cache the search index", "- The index is rebuilt nightly.", HASH)
    val item = InboxItem(
        "i-00000001", ItemKind.MEMORY, ItemState.OPEN, "Cache the search index proposes to add to the workspace memory", "work",
        "Cache the search index", 9, task = "t-cache1", memory = memory,
        answers = listOf(InboxAction.APPROVE, InboxAction.REJECT, InboxAction.LATER),
    )
    var refusal: ErrorBody? = null

    init {
        capabilities = listOf("task-records", "work:$level")
    }

    override fun answer(req: Request) {
        when (req) {
            is Request.ListInbox -> {
                emit(Message.InboxList(req.id, emptyList(), items = listOf(item)))
                emit(Message.Result(req.id))
            }
            is Request.UpdateInbox -> {
                if (req.action == InboxAction.APPROVE && req.hash != memory.hash) {
                    return emit(Message.Result(req.id, ErrorBody(ErrorCode.BAD_REQUEST, "approve on a memory item needs the hash of the text shown")))
                }
                refusal?.let { return emit(Message.Result(req.id, it)) }
                emit(Message.InboxSaved(req.id, item = item.copy(state = ItemState.ANSWERED, answer = req.action, answers = emptyList())))
                emit(Message.Result(req.id))
            }
            else -> super.answer(req)
        }
    }

    companion object {
        const val HASH = "sha256:5d1c0a3c8f8e1a7f1f2b6d1b0b8f0c7f4e1e2d3c4b5a69788766554433221100"
    }
}

class MemoryTest {
    private suspend fun connected(bridge: MemoryBridge): PaneSession {
        val session = PaneSession(oneThread())
        session.connect {
            bridge.start()
            bridge
        }
        session.loadWork()
        return session
    }

    private suspend fun refusal(block: suspend () -> Unit): PaneSession.Failure {
        try {
            block()
        } catch (e: PaneSession.Failure) {
            return e
        }
        fail("expected a refusal")
        throw IllegalStateException()
    }

    private fun MemoryBridge.approves() = received.filterIsInstance<Request.UpdateInbox>().filter { it.action == InboxAction.APPROVE }

    @Test
    fun approveSendsTheHashOfTheTextShown() = runBlocking {
        val bridge = MemoryBridge(14)
        val session = connected(bridge)
        assertTrue(session.state.value.canKeepMemory)
        val saved = session.approveMemory(bridge.item, bridge.memory)
        assertEquals(listOf(MemoryBridge.HASH), bridge.approves().map { it.hash })
        assertEquals(MemoryState.ADDED, saved.memoryState)
    }

    @Test
    fun noApproveOnAMemoryItemWithoutItsHash() = runBlocking {
        val bridge = MemoryBridge(14)
        val session = connected(bridge)
        val e = refusal { session.answer(bridge.item, InboxAction.APPROVE) }
        assertEquals(PaneSession.MEMORY_NOT_SHOWN, PaneSession.describe(e))
        assertTrue(bridge.approves().isEmpty())
        // Reject goes as any answer, with no hash.
        session.answer(bridge.item, InboxAction.REJECT)
        assertNull(bridge.received.filterIsInstance<Request.UpdateInbox>().single().hash)
    }

    @Test
    fun aLevel13HostGetsNoApproveOnAMemoryItem() = runBlocking {
        val bridge = MemoryBridge(13)
        val session = connected(bridge)
        assertFalse(session.state.value.canKeepMemory)
        val e = refusal { session.approveMemory(bridge.item, bridge.memory) }
        assertEquals(PaneSession.MEMORY_UNSUPPORTED, PaneSession.describe(e))
        assertTrue(bridge.approves().isEmpty())
        assertEquals(listOf(InboxAction.REJECT, InboxAction.LATER), bridge.item.memoryOffered(canKeepMemory = false).answers)
    }

    @Test
    fun aFullFileIsTheHostsReasonWithTheSizes() = runBlocking {
        val bridge = MemoryBridge(14)
        val session = connected(bridge)
        bridge.refusal = ErrorBody(ErrorCode.CONFLICT, "WORKSPACE.md would be 16500 bytes; the cap is 16 KiB")
        val full = refusal { session.approveMemory(bridge.item, bridge.memory) }
        assertTrue(full is PaneSession.Failure.Refused)
        assertTrue(PaneSession.describe(full).contains("16 KiB"))
    }

    @Test
    fun onlyAMemoryItemWithItsTextOffersApprove() {
        val bridge = MemoryBridge(14)
        assertEquals(bridge.item.answers, bridge.item.memoryOffered(canKeepMemory = true).answers)
        assertFalse(InboxAction.APPROVE in bridge.item.copy(memory = null).memoryOffered(canKeepMemory = true).answers)
        val other = bridge.item.copy(kind = ItemKind.REVIEW, memory = null)
        assertEquals(other.answers, other.memoryOffered(canKeepMemory = false).answers)
    }

    @Test
    fun theTaskDetailLineFollowsTheItem() {
        val item = MemoryBridge(14).item
        assertEquals(item, listOf(item).memoryItemOf("t-cache1"))
        assertNull(listOf(item.copy(kind = ItemKind.REVIEW)).memoryItemOf("t-cache1"))
        assertEquals("Proposed to the workspace memory · Waits for your approval in the Inbox", item.memoryState.line)
        assertEquals(MemoryState.MOVED_DOWN, item.copy(laterAt = 5).memoryState)
        assertEquals("Added to the workspace memory", item.copy(state = ItemState.ANSWERED, answer = InboxAction.APPROVE).memoryState.line)
        assertEquals(MemoryState.REJECTED, item.copy(state = ItemState.ANSWERED, answer = InboxAction.REJECT).memoryState)
        assertEquals(MemoryState.CLOSED, item.copy(state = ItemState.RESOLVED).memoryState)
    }

    @Test
    fun theWorkspaceModeShowsOnlyFromALevel14Host() {
        val workspaces = listOf(WorkspaceInfo("work", memory = MemoryMode.REVIEW, memoryAt = 5), WorkspaceInfo("old"))
        assertEquals(5L, workspaces.memoryOf("work")?.memoryAt)
        assertNull(workspaces.memoryOf("old"))
        assertNull(workspaces.memoryOf("none"))
        assertEquals("Workspace memory: unknown", MemoryMode("ask").label)
    }
}
