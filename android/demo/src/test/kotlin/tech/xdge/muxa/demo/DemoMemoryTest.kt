package tech.xdge.muxa.demo

import kotlinx.coroutines.asCoroutineDispatcher
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import tech.xdge.muxa.protocol.ErrorCode
import tech.xdge.muxa.protocol.InboxAction
import tech.xdge.muxa.protocol.ItemKind
import tech.xdge.muxa.protocol.ItemState
import tech.xdge.muxa.protocol.MemoryMode
import tech.xdge.muxa.protocol.Message
import tech.xdge.muxa.protocol.Request
import tech.xdge.muxa.protocol.TaskStatus
import tech.xdge.muxa.protocol.WORK_MEMORY
import tech.xdge.muxa.protocol.WORK_WRITES
import tech.xdge.muxa.protocol.Wire
import tech.xdge.muxa.session.PaneSession
import java.util.concurrent.Executors

/** The demo's workspace memory at `work` level 14, as the UI tests use it (ADR 0053). */
class DemoMemoryTest {
    private suspend fun connected(bridge: DemoBridge): PaneSession {
        val session = PaneSession(Executors.newSingleThreadExecutor().asCoroutineDispatcher())
        session.connect { bridge }
        session.loadTasks()
        return session
    }

    private fun bridge(level: Int = WORK_MEMORY, set: DemoBridge.Content = DemoBridge.Content.TESTS) =
        DemoBridge(set, workLevel = level, assistPace = AssistPace.NEVER)

    /** Sends [req] past the session's own checks, as an older app would; returns what came until its `result`. */
    private suspend fun DemoBridge.call(req: Request): List<Message> {
        send(Wire.encode(req))
        val answered = mutableListOf<Message>()
        while (true) {
            val m = Wire.decodeMessage(lines.receive())
            answered += m
            if (m is Message.Result && m.id == req.id) return answered
        }
    }

    private fun PaneSession.item(id: String) = state.value.inboxItems.first { it.id == id }

    @Test
    fun levelFourteenListsTheModesAndADoneTasksMemoryItem() = runBlocking {
        val s = connected(bridge()).state.value
        val workspaces = s.tasks?.workspaces.orEmpty()
        assertEquals(listOf(MemoryMode.REVIEW, MemoryMode.AUTO, MemoryMode.OFF), listOf("work", "personal", "notes").map { n -> workspaces.first { it.name == n }.memory })
        assertTrue(workspaces.first { it.name == "work" }.memoryAt != null)
        val item = s.inboxItems.single { it.kind == ItemKind.MEMORY }
        assertEquals(TaskStatus.DONE, s.tasks?.records?.first { it.id == item.task }?.status)
        assertEquals(listOf(InboxAction.APPROVE, InboxAction.REJECT, InboxAction.LATER), item.answers)
        val text = item.memory!!.text
        assertEquals(50, text.lines().size)
        assertTrue(text.toByteArray().size in 4_000..4_096)
    }

    @Test
    fun theDemoRefusesAnApproveWithoutTheShownHash() = runBlocking {
        val bridge = bridge()
        for ((id, hash) in listOf(2L to null, 3L to "sha256:00")) {
            val error = (bridge.call(Request.UpdateInbox(id, ITEM, InboxAction.APPROVE, hash = hash)).last() as Message.Result).error
            assertEquals(ErrorCode.BAD_REQUEST, error?.code)
            assertEquals(DemoMemory.NOT_SHOWN, error?.message)
        }
        assertEquals("", bridge.memoryFile("work"))
        bridge.close()
    }

    @Test
    fun approveWithTheHashAppendsOnce() = runBlocking {
        val bridge = bridge()
        val session = connected(bridge)
        val item = session.item(ITEM)
        val before = session.state.value.tasks?.workspaces?.first { it.name == "work" }?.memoryAt
        val saved = session.approveMemory(item, item.memory!!)
        assertEquals(ItemState.ANSWERED to InboxAction.APPROVE, saved.state to saved.answer)
        val file = bridge.memoryFile("work")
        assertTrue(file.startsWith("## Cache the search results (t-memo01, "))
        assertTrue(file.contains(item.memory!!.text))
        // Approve refreshed the Tasks: the file's time moved.
        assertTrue(session.state.value.tasks?.workspaces?.first { it.name == "work" }?.memoryAt!! > before!!)
        session.approveMemory(session.item(ITEM), item.memory!!)
        assertEquals(file, bridge.memoryFile("work"))
    }

    @Test
    fun rejectWritesNothing() = runBlocking {
        val bridge = bridge()
        val session = connected(bridge)
        session.answer(session.item(ITEM), InboxAction.REJECT)
        assertEquals(InboxAction.REJECT, session.item(ITEM).answer)
        assertEquals("", bridge.memoryFile("work"))
    }

    @Test
    fun levelThirteenHasNoMemory() = runBlocking {
        val s = connected(bridge(WORK_WRITES)).state.value
        assertTrue(s.inboxItems.none { it.kind == ItemKind.MEMORY })
        assertTrue(s.tasks?.workspaces.orEmpty().all { it.memory == null && it.memoryAt == null })
        assertTrue(s.tasks?.records.orEmpty().none { it.id == "t-memo01" })
    }

    @Test
    fun theShowcaseProposesAndTheHashIsTheProtocols() = runBlocking {
        val s = connected(bridge(set = DemoBridge.Content.SHOWCASE)).state.value
        val item = s.inboxItems.single { it.kind == ItemKind.MEMORY }
        assertEquals(TaskStatus.DONE, s.tasks?.records?.first { it.id == item.task }?.status)
        // The protocol fixture's memory item (fixtures/items/messages.jsonl, line 8).
        assertEquals(
            "sha256:4f4f83d8f4e4547bd05edb2c47b11522c43ac7999f3b8f97a0e14143832aba75",
            DemoMemory.hash(
                "t-00b001", "Rotate the API keys",
                "- API keys live in Vault under kv/work/api; rotate them with `make rotate-keys`.\n" +
                    "- Decision: keys rotate every 90 days, because the vendor expires them at 100.\n",
            ),
        )
    }

    private companion object {
        const val ITEM = "i-0000e101"
    }
}
