package tech.xdge.muxa.session

import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test
import tech.xdge.muxa.protocol.ChatPrompt
import tech.xdge.muxa.protocol.ErrorBody
import tech.xdge.muxa.protocol.ErrorCode
import tech.xdge.muxa.protocol.InboxAction
import tech.xdge.muxa.protocol.InboxItem
import tech.xdge.muxa.protocol.ItemKind
import tech.xdge.muxa.protocol.ItemState
import tech.xdge.muxa.protocol.Message
import tech.xdge.muxa.protocol.Request
import tech.xdge.muxa.protocol.TaskRecord
import tech.xdge.muxa.protocol.TaskStatus

private val permission = ChatPrompt(
    "toolu_01", ChatPrompt.Kind.PERMISSION,
    options = listOf(ChatPrompt.Option("Yes", listOf("Enter")), ChatPrompt.Option("No", listOf("Escape"), cancel = true)),
)

private val question = ChatPrompt(
    "toolu_02", ChatPrompt.Kind.QUESTION, "Which format?",
    listOf(ChatPrompt.Option("TOML", listOf("1")), ChatPrompt.Option("JSON", listOf("2"))),
)

private fun item(
    id: String, kind: ItemKind, answers: List<InboxAction>, state: ItemState = ItemState.OPEN,
    task: String? = null, prompt: ChatPrompt? = null, readAt: Long? = null, until: Long? = null,
) = InboxItem(
    id, kind, state, "Question $id", "work", "Title $id", createdAt = 100, readAt = readAt, until = until,
    task = task, pane = task?.let { "%1" }, prompt = prompt, answers = answers,
)

/** A host with `work` level 3, answering items as PROTOCOL.md "Inbox items" says. */
private class ItemsBridge : FakeBridge() {
    var items = listOf(
        item("i-00000001", ItemKind.ASK, listOf(InboxAction.APPROVE, InboxAction.REJECT, InboxAction.LATER), task = "t-1", prompt = permission),
        item(
            "i-00000002", ItemKind.REVIEW,
            listOf(InboxAction.ACCEPT, InboxAction.REDO, InboxAction.CHANGE_COURSE, InboxAction.KEEP_OPEN, InboxAction.LATER),
            task = "t-2", readAt = 50,
        ),
        item("i-00000003", ItemKind.REVIEW, emptyList(), ItemState.ANSWERED, task = "t-3"),
    )

    /** Set: the next answer is refused with this code. */
    var refuse: ErrorCode? = null
    private val _asked = mutableListOf<Request>()
    val asked: List<Request> get() = synchronized(_asked) { _asked.toList() }

    override fun answer(req: Request) {
        when (req) {
            is Request.ListInbox -> {
                record(req)
                emit(Message.InboxList(req.id, emptyList(), items))
                emit(Message.Result(req.id))
            }
            is Request.UpdateInbox -> update(req)
            else -> super.answer(req)
        }
    }

    private fun update(req: Request.UpdateInbox) {
        record(req)
        refuse?.let {
            refuse = null
            return emit(Message.Result(req.id, ErrorBody(it, "refused by the host")))
        }
        val current = items.first { it.id == req.entry }
        val saved = when (req.action) {
            InboxAction.READ -> current.copy(readAt = 200)
            InboxAction.LATER -> current.copy(state = ItemState.LATER, until = req.until)
            else -> current.copy(state = ItemState.ANSWERED, answer = req.action, answers = emptyList())
        }
        if (req.action == InboxAction.ACCEPT) {
            tasks = tasks.map { if (it.id == current.task) it.copy(rev = it.rev + 1, status = TaskStatus.DONE) else it }
        }
        items = items.map { if (it.id == saved.id) saved else it }
        emit(Message.InboxSaved(req.id, item = saved))
        emit(Message.Result(req.id))
    }

    private fun record(req: Request) = synchronized(_asked) { _asked += req }
}

class ItemsTest {
    private suspend fun connected(bridge: ItemsBridge): PaneSession {
        val session = PaneSession(oneThread())
        session.connect {
            bridge.start()
            bridge
        }
        return session
    }

    private fun bridge(level: Int = 3) = ItemsBridge().apply {
        capabilities = listOf("task-records", "work:$level")
        tasks = listOf(TaskRecord("t-2", 1, "work", "Title t-2", "Goal", TaskStatus.OPEN, 1, 1))
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

    @Test
    fun itemsAreListedInTheHostsOrderAtWorkLevelThree() = runBlocking {
        val s = connected(bridge()).state.value
        assertTrue(s.canAnswerItems)
        assertEquals(listOf("i-00000001", "i-00000002", "i-00000003"), s.inboxItems.map { it.id })
        // The answered one is kept for the record, but nothing waits on it.
        assertEquals(listOf("i-00000001", "i-00000002"), s.inboxItems.waiting(now = 300).map { it.id })
    }

    @Test
    fun anOlderHostKeepsItsInboxAndIsNeverAskedForAnAnswer() = runBlocking {
        val bridge = bridge(level = 2)
        val session = connected(bridge)
        assertTrue(session.state.value.inboxItems.isEmpty())
        val e = refusal { session.answer(bridge.items[0], InboxAction.APPROVE) }
        assertEquals(PaneSession.ITEMS_UNSUPPORTED, PaneSession.describe(e))
        session.markRead(bridge.items[0])
        assertTrue(bridge.asked.none { it is Request.UpdateInbox })
    }

    @Test
    fun anAnswerSendsItsTextAndKeepsTheSavedItem() = runBlocking {
        val bridge = bridge()
        val session = connected(bridge)
        val review = session.state.value.inboxItems[1]
        val saved = session.answer(review, InboxAction.REDO, text = "Keep the old export.")
        assertEquals(ItemState.ANSWERED, saved.state)
        val sent = bridge.asked.filterIsInstance<Request.UpdateInbox>().last()
        assertEquals(Request.UpdateInbox(sent.id, "i-00000002", InboxAction.REDO, text = "Keep the old export."), sent)
        assertEquals(listOf("i-00000001"), session.state.value.inboxItems.waiting(now = 300).map { it.id })
    }

    @Test
    fun chooseSendsTheOptionsIndex() = runBlocking {
        val bridge = bridge().apply {
            items = listOf(item("i-00000004", ItemKind.ASK, listOf(InboxAction.CHOOSE, InboxAction.LATER), task = "t-4", prompt = question))
        }
        val session = connected(bridge)
        session.answer(session.state.value.inboxItems[0], InboxAction.CHOOSE, option = 1)
        val sent = bridge.asked.filterIsInstance<Request.UpdateInbox>().last()
        assertEquals(1, sent.option)
        assertNull(sent.text)
    }

    @Test
    fun acceptReadsTheTasksAgain() = runBlocking {
        val bridge = bridge()
        val session = connected(bridge)
        session.answer(session.state.value.inboxItems[1], InboxAction.ACCEPT)
        assertEquals(TaskStatus.DONE, session.state.value.tasks?.record("t-2")?.status)
    }

    @Test
    fun aChangedMenuIsAConflictAndTheListIsReadAgain() = runBlocking {
        val bridge = bridge()
        val session = connected(bridge)
        val lists = bridge.asked.count { it is Request.ListInbox }
        bridge.refuse = ErrorCode.CONFLICT
        val e = refusal { session.answer(session.state.value.inboxItems[0], InboxAction.APPROVE) }
        assertTrue(e is PaneSession.Failure.ItemChanged)
        assertEquals(PaneSession.ITEM_CHANGED, PaneSession.describe(e))
        assertEquals(lists + 1, bridge.asked.count { it is Request.ListInbox })
    }

    @Test
    fun aBadRequestShowsTheHostsReason() = runBlocking {
        val bridge = bridge()
        val session = connected(bridge)
        bridge.refuse = ErrorCode.BAD_REQUEST
        val e = refusal { session.answer(session.state.value.inboxItems[1], InboxAction.ACCEPT) }
        assertEquals("refused by the host", PaneSession.describe(e))
    }

    @Test
    fun openingAnUnreadItemMarksItReadOnce() = runBlocking {
        val bridge = bridge()
        val session = connected(bridge)
        session.markRead(session.state.value.inboxItems[0])
        assertEquals(200L, session.state.value.inboxItems[0].readAt)
        assertEquals(ItemState.OPEN, session.state.value.inboxItems[0].state)
        // Already read: nothing is sent.
        session.markRead(session.state.value.inboxItems[0])
        session.markRead(session.state.value.inboxItems[1])
        assertEquals(1, bridge.asked.count { it is Request.UpdateInbox && it.action == InboxAction.READ })
    }

    @Test
    fun theButtonsFollowTheKind() {
        val all = { kind: ItemKind, answers: List<InboxAction> -> item("i-1", kind, answers).buttons }
        val review = all(ItemKind.REVIEW, listOf(InboxAction.ACCEPT, InboxAction.REDO, InboxAction.CHANGE_COURSE, InboxAction.KEEP_OPEN, InboxAction.LATER))
        assertEquals(listOf(InboxAction.ACCEPT, InboxAction.REDO), review.main)
        assertEquals(listOf(InboxAction.CHANGE_COURSE, InboxAction.KEEP_OPEN, InboxAction.LATER), review.more)
        val failure = all(ItemKind.FAILURE, listOf(InboxAction.RETRY, InboxAction.CHANGE_COURSE, InboxAction.DROP, InboxAction.DISMISS, InboxAction.LATER))
        assertEquals(listOf(InboxAction.RETRY, InboxAction.CHANGE_COURSE), failure.main)
        assertEquals(listOf(InboxAction.DROP, InboxAction.DISMISS, InboxAction.LATER), failure.more)
        assertEquals(listOf(InboxAction.DISMISS), all(ItemKind.BRIEFING, listOf(InboxAction.DISMISS, InboxAction.LATER)).main)
        // An unknown kind shows what the host offers.
        assertEquals(listOf(InboxAction.DISMISS), all(ItemKind("digest"), listOf(InboxAction.DISMISS, InboxAction.LATER)).main)
    }

    @Test
    fun theMainAnswerNeverNeedsAText() {
        val ask = item("i-1", ItemKind.ASK, listOf(InboxAction.APPROVE, InboxAction.REJECT, InboxAction.LATER), prompt = permission)
        assertEquals(InboxAction.APPROVE, ask.mainAnswer)
        val review = item("i-2", ItemKind.REVIEW, listOf(InboxAction.ACCEPT, InboxAction.REDO, InboxAction.LATER))
        assertEquals(InboxAction.ACCEPT, review.mainAnswer)
        val failure = item("i-3", ItemKind.FAILURE, listOf(InboxAction.RETRY, InboxAction.CHANGE_COURSE, InboxAction.LATER))
        assertEquals(InboxAction.RETRY, failure.mainAnswer)
        // A question is answered by its options, so it has none.
        val asked = item("i-4", ItemKind.ASK, listOf(InboxAction.CHOOSE, InboxAction.REJECT, InboxAction.LATER), prompt = question)
        assertNull(asked.mainAnswer)
        assertEquals(listOf("TOML", "JSON"), asked.choices.map { it.label })
        assertTrue(ask.choices.isEmpty())
    }

    @Test
    fun aLaterItemWaitsAgainOnceItsTimePassed() {
        val later = item("i-1", ItemKind.REVIEW, listOf(InboxAction.ACCEPT), ItemState.LATER, until = 500)
        assertEquals(false, later.isWaiting(now = 400))
        assertEquals(true, later.isWaiting(now = 500))
        assertEquals(false, item("i-2", ItemKind.ASK, emptyList(), ItemState.RESOLVED).isWaiting(now = 0))
    }
}
