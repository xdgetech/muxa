package tech.xdge.muxa.session

import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test
import tech.xdge.muxa.protocol.AssistResult
import tech.xdge.muxa.protocol.AssistScope
import tech.xdge.muxa.protocol.AssistantInfo
import tech.xdge.muxa.protocol.AssistantRequest
import tech.xdge.muxa.protocol.DelegateInfo
import tech.xdge.muxa.protocol.Delivery
import tech.xdge.muxa.protocol.ErrorBody
import tech.xdge.muxa.protocol.ErrorCode
import tech.xdge.muxa.protocol.InboxAction
import tech.xdge.muxa.protocol.InboxItem
import tech.xdge.muxa.protocol.ItemKind
import tech.xdge.muxa.protocol.ItemState
import tech.xdge.muxa.protocol.Message
import tech.xdge.muxa.protocol.OverallRequests
import tech.xdge.muxa.protocol.Request
import tech.xdge.muxa.protocol.ResultWrite
import tech.xdge.muxa.protocol.Share
import tech.xdge.muxa.protocol.TaskRecord
import tech.xdge.muxa.protocol.TaskStatus
import tech.xdge.muxa.protocol.WriteProposal
import tech.xdge.muxa.protocol.WriterInfo
import tech.xdge.muxa.protocol.Writes

/**
 * A host at [level] with one `write` item. Approve answers as the host does: `bad-request` without
 * the write's hash; [refusal] answers it otherwise, as a budget's `conflict` does.
 */
private class WriteBridge(level: Int) : FakeBridge() {
    val write = WriteProposal("q-00000001", 1, null, "work-writer", "Post to #release", "slack:#release", "Release 2.4 is out.", HASH)
    val item = InboxItem(
        "i-00000001", ItemKind.WRITE, ItemState.OPEN, "work-writer proposes to write to slack:#release", "work", "Post to #release", 9,
        summary = "The release is tagged.", write = write, answers = listOf(InboxAction.APPROVE, InboxAction.REJECT, InboxAction.LATER),
    )
    var refusal: ErrorBody? = null

    init {
        capabilities = listOf("task-records", "work:$level")
    }

    override fun answer(req: Request) {
        when (req) {
            is Request.ListInbox -> {
                emit(Message.InboxList(req.id, emptyList(), items = listOf(item), assistant = INFO))
                emit(Message.Result(req.id))
            }
            is Request.UpdateInbox -> {
                if (req.action == InboxAction.APPROVE && req.hash != write.hash) {
                    return emit(Message.Result(req.id, ErrorBody(ErrorCode.BAD_REQUEST, "this app cannot show the whole write; update the app or use muxa-cli")))
                }
                refusal?.let { return emit(Message.Result(req.id, it)) }
                val task = "t-writer1".takeIf { req.action == InboxAction.APPROVE }
                task?.let { emit(Message.TaskSaved(req.id, TaskRecord(it, 1, "work", write.action, "goal", TaskStatus.OPEN, 1, 1, worker = "work-sender"))) }
                emit(Message.InboxSaved(req.id, item = item.copy(state = ItemState.ANSWERED, answer = req.action, answers = emptyList(), task = task)))
                emit(Message.Result(req.id))
            }
            else -> super.answer(req)
        }
    }

    companion object {
        const val HASH = "sha256:1f92cb95d7657bfda393b3886d66d7ba4720e93da90eeaf38bc181b7918d769f"
        val INFO = AssistantInfo(
            "secretary",
            listOf(
                DelegateInfo("work", Share.SUMMARIES, OverallRequests.REVIEW, writer = WriterInfo("work-sender", "Posts approved messages."), writes = Writes.REVIEW),
                DelegateInfo("personal", Share.OUTLINE, OverallRequests.REVIEW, writes = Writes.NONE),
                DelegateInfo("notes", Share.OUTLINE, OverallRequests.REVIEW),
            ),
        )
    }
}

class WriteTest {
    private suspend fun connected(bridge: WriteBridge): PaneSession {
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

    private fun WriteBridge.approves() = received.filterIsInstance<Request.UpdateInbox>().filter { it.action == InboxAction.APPROVE }

    @Test
    fun approveSendsTheHashOfTheWriteShown() = runBlocking {
        val bridge = WriteBridge(13)
        val session = connected(bridge)
        assertTrue(session.state.value.canWrite)
        val saved = session.approveWrite(bridge.item, bridge.write)
        assertEquals(listOf(WriteBridge.HASH), bridge.approves().map { it.hash })
        assertEquals("t-writer1", saved.task)
        assertEquals(ItemState.ANSWERED, saved.state)
    }

    @Test
    fun noApproveOnAWriteWithoutItsHash() = runBlocking {
        val bridge = WriteBridge(13)
        val session = connected(bridge)
        val e = refusal { session.answer(bridge.item, InboxAction.APPROVE) }
        assertEquals(PaneSession.WRITE_NOT_SHOWN, PaneSession.describe(e))
        assertTrue(bridge.approves().isEmpty())
        // Reject and Later go as any answer, with no hash; Reject keeps its reason.
        session.answer(bridge.item, InboxAction.REJECT, text = "Wrong channel")
        val sent = bridge.received.filterIsInstance<Request.UpdateInbox>().single()
        assertEquals("Wrong channel", sent.text)
        assertNull(sent.hash)
    }

    @Test
    fun aLevel12HostGetsNoApproveOnAWrite() = runBlocking {
        val bridge = WriteBridge(12)
        val session = connected(bridge)
        assertFalse(session.state.value.canWrite)
        val e = refusal { session.approveWrite(bridge.item, bridge.write) }
        assertEquals(PaneSession.WRITES_UNSUPPORTED, PaneSession.describe(e))
        assertTrue(bridge.approves().isEmpty())
        // The item screen offers no Approve there: the content is not shown in a write view.
        assertEquals(listOf(InboxAction.REJECT, InboxAction.LATER), bridge.item.offered(canWrite = false).answers)
    }

    @Test
    fun anotherHashIsTheHostsRefusalAndABudgetItsReason() = runBlocking {
        val bridge = WriteBridge(13)
        val session = connected(bridge)
        val other = bridge.write.copy(content = "Something else.", hash = "sha256:00")
        val wrong = refusal { session.approveWrite(bridge.item, other) }
        assertTrue(PaneSession.describe(wrong).contains("cannot show the whole write"))
        bridge.refusal = ErrorBody(ErrorCode.CONFLICT, "max_writes_per_day (10) reached; it resets at midnight")
        val budget = refusal { session.approveWrite(bridge.item, bridge.write) }
        assertTrue(budget is PaneSession.Failure.Refused)
        assertEquals("max_writes_per_day (10) reached; it resets at midnight", PaneSession.describe(budget))
    }

    @Test
    fun onlyAWriteItemWithItsWriteOffersApprove() {
        val bridge = WriteBridge(13)
        assertEquals(bridge.item.answers, bridge.item.offered(canWrite = true).answers)
        assertFalse(InboxAction.APPROVE in bridge.item.copy(write = null).offered(canWrite = true).answers)
        val other = bridge.item.copy(kind = ItemKind.HANDOFF, write = null)
        assertEquals(other.answers, other.offered(canWrite = false).answers)
    }

    @Test
    fun workDetailNamesEachWritesState() {
        val w = ResultWrite("Post", "slack:#a", "x", "sha256:1", item = "i-1")
        val open = InboxItem("i-1", ItemKind.WRITE, ItemState.OPEN, "q", "work", "Post", 1)
        assertEquals(WriteState.PROPOSED, w.state(emptyList()))
        assertEquals(WriteState.WAITING, w.state(listOf(open)))
        assertEquals(WriteState.REJECTED, w.state(listOf(open.copy(state = ItemState.ANSWERED, answer = InboxAction.REJECT))))
        assertEquals(WriteState.CLOSED, w.state(listOf(open.copy(state = ItemState.RESOLVED))))
        assertEquals(WriteState.STARTED, w.state(listOf(open.copy(state = ItemState.ANSWERED, answer = InboxAction.APPROVE, task = "t-1"))))
        assertEquals("t-2", w.copy(task = "t-2").taskOf(emptyList()))
    }

    @Test
    fun aWriteFromAnOlderVersionSaysSo() {
        val q = AssistantRequest(
            "q-00000001", "w-1", 2, "work", AssistScope.DRAFT, Delivery.RESULTED, 5, 1, 5,
            results = listOf(AssistResult(2, 5, "New."), AssistResult(1, 3, "Old.", stale = true)),
        )
        val write = WriteBridge(13).write
        assertTrue(write.isOlder(listOf(q)))
        assertFalse(write.copy(version = 2).isOlder(listOf(q)))
        assertFalse(write.isOlder(emptyList()))
        assertTrue(q.results.last().isOlder(q))
        assertEquals("work-writer", write.proposer("work"))
        assertEquals(delegateName("work"), write.copy(worker = null).proposer("work"))
    }

    @Test
    fun theWriterAndItsModeShowWhereStaffShow() {
        val info = WriteBridge.INFO
        assertEquals("work-sender", info.writesOf("work")?.writer?.worker)
        assertEquals(Writes.NONE, info.writesOf("personal")?.writes)
        // A host before level 13 sends neither: nothing to show.
        assertNull(info.writesOf("notes"))
        assertNull((null as AssistantInfo?).writesOf("work"))
        assertEquals("Writes: unknown", Writes("auto").label)
    }
}
