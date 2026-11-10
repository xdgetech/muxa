package tech.xdge.muxa.demo

import kotlinx.coroutines.asCoroutineDispatcher
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import tech.xdge.muxa.protocol.ErrorCode
import tech.xdge.muxa.protocol.InboxAction
import tech.xdge.muxa.protocol.ItemKind
import tech.xdge.muxa.protocol.ItemState
import tech.xdge.muxa.protocol.Message
import tech.xdge.muxa.protocol.Request
import tech.xdge.muxa.protocol.WORK_STAFF
import tech.xdge.muxa.protocol.WORK_WRITES
import tech.xdge.muxa.protocol.Wire
import tech.xdge.muxa.protocol.Writes
import tech.xdge.muxa.session.PaneSession
import java.util.concurrent.Executors

/** The demo's write proposals at `work` level 13, as the UI tests use them (ADR 0052). */
class DemoWritesTest {
    private suspend fun connected(level: Int = WORK_WRITES, set: DemoBridge.Content = DemoBridge.Content.TESTS): PaneSession {
        val session = PaneSession(Executors.newSingleThreadExecutor().asCoroutineDispatcher())
        session.connect { DemoBridge(set, workLevel = level, assistPace = AssistPace.NEVER) }
        return session
    }

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

    private fun PaneSession.writerTasks() = state.value.tasks?.records.orEmpty().filter { it.worker == "work-sender" }

    @Test
    fun levelThirteenListsTheWriterAndTwoWriteItems() = runBlocking {
        val s = connected().state.value
        val work = s.assistant?.delegates?.first { it.workspace == "work" }
        assertEquals("work-sender", work?.writer?.worker)
        assertEquals(Writes.REVIEW, work?.writes)
        assertEquals(Writes.NONE, s.assistant?.delegates?.first { it.workspace == "personal" }?.writes)
        val writes = s.inboxItems.filter { it.kind == ItemKind.WRITE }
        assertEquals(listOf(POST, COMMENT), writes.map { it.id }.sorted())
        val q = s.requests.first { it.id == REQUEST }
        assertEquals(listOf(listOf(POST), listOf(COMMENT)), q.results.map { r -> r.writes.map { it.item } })
        assertEquals(1L, s.inboxItems.first { it.id == COMMENT }.write?.version)
    }

    @Test
    fun theDemoRefusesAnApproveWithoutTheShownHash() = runBlocking {
        val bridge = DemoBridge(workLevel = WORK_WRITES, assistPace = AssistPace.NEVER)
        // An app before level 13 sends no hash; another hash is not the write shown either.
        for ((id, hash) in listOf(2L to null, 3L to "sha256:00")) {
            val answer = bridge.call(Request.UpdateInbox(id, POST, InboxAction.APPROVE, hash = hash))
            val error = (answer.last() as Message.Result).error
            assertEquals(ErrorCode.BAD_REQUEST, error?.code)
            assertEquals(DemoWrites.NOT_SHOWN, error?.message)
            assertTrue(answer.none { it is Message.TaskSaved })
        }
        val listed = bridge.call(Request.ListInbox(4)).filterIsInstance<Message.InboxList>().single()
        assertEquals(ItemState.OPEN, listed.items.first { it.id == POST }.state)
        assertTrue(bridge.call(Request.ListTasks(5)).filterIsInstance<Message.TaskList>().single().entries.none { it.worker == "work-sender" })
        bridge.close()
    }

    @Test
    fun approveWithTheHashStartsOneTaskOfTheWriter() = runBlocking {
        val session = connected()
        val item = session.item(POST)
        val saved = session.approveWrite(item, item.write!!)
        session.loadTasks()
        val task = session.writerTasks().single()
        assertEquals(task.id, saved.task)
        assertEquals("w-0000b006", task.work)
        assertEquals("Post to #release", task.title)
        assertTrue(task.goal.contains(item.write!!.content))
        assertEquals("muxa-write: $POST ${item.write!!.hash}", task.goal.lines().last())
        assertEquals(task.id, session.state.value.requests.first { it.id == REQUEST }.results.first().writes.single().task)
        // A second approve starts nothing and returns the same Task.
        val again = session.approveWrite(session.item(POST), item.write!!)
        session.loadTasks()
        assertEquals(task.id, again.task)
        assertEquals(1, session.writerTasks().size)
    }

    @Test
    fun rejectStartsNothing() = runBlocking {
        val session = connected()
        session.answer(session.item(COMMENT), InboxAction.REJECT, text = "Not in this ticket")
        session.loadTasks()
        assertEquals(InboxAction.REJECT, session.item(COMMENT).answer)
        assertTrue(session.writerTasks().isEmpty())
    }

    @Test
    fun levelTwelveHasNoWritesAndNoWriter() = runBlocking {
        val s = connected(WORK_STAFF).state.value
        assertTrue(s.inboxItems.none { it.kind == ItemKind.WRITE })
        assertTrue(s.requests.none { it.id == REQUEST })
        assertTrue(s.assistant?.delegates.orEmpty().all { it.writer == null && it.writes == null })
    }

    @Test
    fun theShowcaseHasTwoWritesAndTheHashIsTheRoutinesFunction() = runBlocking {
        val s = connected(set = DemoBridge.Content.SHOWCASE).state.value
        assertEquals(2, s.inboxItems.count { it.kind == ItemKind.WRITE && it.state == ItemState.OPEN })
        // The protocol fixture's write (fixtures/assistant/messages.jsonl, line 55).
        assertEquals(
            "sha256:e495150141fce15c0e08a8f84d0164f0eec6531c80e3405b5281cf4a871014c5",
            DemoWrites.hash("Comment on ENG-12", "linear:ENG-12", "The fix ships in 2.4."),
        )
        val long = DemoWrites.numberedLines()
        assertEquals(100, long.lines().size)
        assertTrue(long.toByteArray().size in 8_000..8_192)
        assertNull(s.inboxItems.firstOrNull { it.kind == ItemKind.WRITE && it.write == null })
    }

    companion object {
        private const val REQUEST = "q-0000a003"
        private const val POST = "i-0000d101"
        private const val COMMENT = "i-0000d102"
    }
}
