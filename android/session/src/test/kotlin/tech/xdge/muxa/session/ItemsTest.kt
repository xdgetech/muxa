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
        if (req.action == InboxAction.ASK_AGENT) {
            if (req.text.isNullOrEmpty()) return emit(Message.Result(req.id, ErrorBody(ErrorCode.BAD_REQUEST, "a text is needed")))
            emit(Message.SessionStarted(req.id, "$9", "ask-work"))
            emit(Message.InboxSaved(req.id, item = current))
            return emit(Message.Result(req.id))
        }
        val saved = when (req.action) {
            InboxAction.READ -> current.copy(readAt = 200)
            InboxAction.LATER -> current.copy(laterAt = 60)
            else -> current.copy(state = ItemState.ANSWERED, answer = req.action, answers = emptyList())
        }
        if (req.action == InboxAction.ACCEPT) {
            tasks = tasks.map { if (it.id == current.task) it.copy(rev = it.rev + 1, status = TaskStatus.DONE) else it }
        }
        items = if (req.action == InboxAction.LATER) {
            // Below the other open items, above the closed ones (ADR 0040).
            val others = items.filter { it.id != saved.id }
            val closed = others.indexOfFirst { it.state != ItemState.OPEN }.takeIf { it >= 0 } ?: others.size
            others.take(closed) + saved + others.drop(closed)
        } else {
            items.map { if (it.id == saved.id) saved else it }
        }
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
    fun laterMovesTheItemDownInTheHostsOrder() = runBlocking {
        val bridge = bridge()
        val session = connected(bridge)
        val saved = session.answer(session.state.value.inboxItems[0], InboxAction.LATER)
        assertEquals(60L, saved.laterAt)
        assertEquals(ItemState.OPEN, saved.state)
        assertNull(bridge.asked.filterIsInstance<Request.UpdateInbox>().last().until)
        // Listed again, moved down, not hidden.
        assertEquals(listOf("i-00000002", "i-00000001"), session.state.value.inboxItems.waiting(now = 300).map { it.id })
    }

    /** ADR 0041: the reply names the workspace's Ask session, and the item stays open. */
    @Test
    fun askAgentNamesTheSessionAndLeavesTheItemOpen() = runBlocking {
        val bridge = bridge().apply {
            items = items.map {
                if (it.id == "i-00000002") it.copy(answers = it.answers.dropLast(1) + InboxAction.ASK_AGENT + InboxAction.LATER) else it
            }
        }
        val session = connected(bridge)
        val started = session.askAgent(session.state.value.inboxItems[1], "Put this in Notion.")
        assertEquals("$9" to "ask-work", started.session to started.name)
        assertEquals(listOf("i-00000001", "i-00000002"), session.state.value.inboxItems.waiting(now = 300).map { it.id })
        val sent = bridge.asked.filterIsInstance<Request.UpdateInbox>().last()
        assertEquals(Request.UpdateInbox(sent.id, "i-00000002", InboxAction.ASK_AGENT, text = "Put this in Notion."), sent)
        // An item without Ask Agent: nothing is sent.
        val e = refusal { session.askAgent(session.state.value.inboxItems[0], "x") }
        assertEquals(PaneSession.ASK_AGENT_UNSUPPORTED, PaneSession.describe(e))
        assertEquals(1, bridge.asked.count { it is Request.UpdateInbox && it.action == InboxAction.ASK_AGENT })
    }

    @Test
    fun theButtonsFollowTheKind() {
        val all = { kind: ItemKind, answers: List<InboxAction> -> item("i-1", kind, answers).buttons }
        val review = all(
            ItemKind.REVIEW,
            listOf(InboxAction.ACCEPT, InboxAction.REPLY, InboxAction.REDO, InboxAction.CHANGE_COURSE, InboxAction.KEEP_OPEN, InboxAction.LATER),
        )
        assertEquals(listOf(InboxAction.ACCEPT, InboxAction.REPLY), review.main)
        assertEquals(listOf(InboxAction.REDO, InboxAction.CHANGE_COURSE), review.more)
        assertEquals(InboxAction.KEEP_OPEN, review.done)
        assertTrue(review.later)
        val failure = all(
            ItemKind.FAILURE,
            listOf(InboxAction.RETRY, InboxAction.REPLY, InboxAction.CHANGE_COURSE, InboxAction.DROP, InboxAction.DISMISS, InboxAction.LATER),
        )
        assertEquals(listOf(InboxAction.RETRY, InboxAction.REPLY), failure.main)
        assertEquals(listOf(InboxAction.CHANGE_COURSE, InboxAction.DROP), failure.more)
        assertEquals(InboxAction.DISMISS, failure.done)
        // Later alone is no More.
        val ask = all(ItemKind.ASK, listOf(InboxAction.APPROVE, InboxAction.REJECT, InboxAction.REPLY, InboxAction.LATER))
        assertEquals(ItemButtons(listOf(InboxAction.APPROVE, InboxAction.REJECT, InboxAction.REPLY), emptyList(), later = true), ask)
        // An ask the app can't answer closes with ✓.
        assertEquals(InboxAction.DISMISS, all(ItemKind.ASK, listOf(InboxAction.DISMISS, InboxAction.LATER)).done)
        // A host before `reply` (ADR 0033 §4a): Redo alone is a button, not a More of one.
        val older = all(ItemKind.REVIEW, listOf(InboxAction.ACCEPT, InboxAction.REDO, InboxAction.KEEP_OPEN, InboxAction.LATER))
        assertEquals(listOf(InboxAction.ACCEPT, InboxAction.REDO), older.main)
        assertTrue(older.more.isEmpty())
        val noSession = all(ItemKind.FAILURE, listOf(InboxAction.DROP, InboxAction.DISMISS, InboxAction.LATER))
        assertEquals(ItemButtons(listOf(InboxAction.DROP), emptyList(), InboxAction.DISMISS, later = true), noSession)
        val briefing = all(ItemKind.BRIEFING, listOf(InboxAction.DISMISS, InboxAction.LATER))
        assertEquals(ItemButtons(emptyList(), emptyList(), InboxAction.DISMISS, later = true), briefing)
        // ADR 0041: Ask Agent is a briefing's and a proposal's button, and under More on a review.
        val asking = item("e-2", ItemKind.BRIEFING, listOf(InboxAction.DISMISS, InboxAction.ASK_AGENT, InboxAction.LATER))
        assertEquals(listOf(InboxAction.ASK_AGENT), asking.buttons.main)
        assertEquals(
            listOf(InboxAction.APPROVE, InboxAction.REJECT, InboxAction.ASK_AGENT),
            all(ItemKind.PROPOSAL, listOf(InboxAction.APPROVE, InboxAction.REJECT, InboxAction.ASK_AGENT, InboxAction.LATER)).main,
        )
        val reviewAsks = all(
            ItemKind.REVIEW,
            listOf(InboxAction.ACCEPT, InboxAction.REPLY, InboxAction.REDO, InboxAction.CHANGE_COURSE, InboxAction.KEEP_OPEN, InboxAction.ASK_AGENT, InboxAction.LATER),
        )
        assertEquals(listOf(InboxAction.REDO, InboxAction.CHANGE_COURSE, InboxAction.ASK_AGENT), reviewAsks.more)
        // An ask shows it as a button: it is the only hidden one.
        val askAsks = all(ItemKind.ASK, listOf(InboxAction.APPROVE, InboxAction.REJECT, InboxAction.REPLY, InboxAction.ASK_AGENT, InboxAction.LATER))
        assertEquals(listOf(InboxAction.APPROVE, InboxAction.REJECT, InboxAction.REPLY, InboxAction.ASK_AGENT), askAsks.main)
        // An unknown kind shows what the host offers.
        assertEquals(listOf(InboxAction.APPROVE), all(ItemKind("digest"), listOf(InboxAction.APPROVE, InboxAction.DISMISS, InboxAction.LATER)).main)
    }

    @Test
    fun theMainAnswerIsTheHostsFirstOrAReply() {
        val ask = item("i-1", ItemKind.ASK, listOf(InboxAction.APPROVE, InboxAction.REJECT, InboxAction.REPLY, InboxAction.LATER), prompt = permission)
        assertEquals(InboxAction.APPROVE, ask.mainAnswer())
        assertEquals(InboxAction.REPLY, ask.mainAnswer("Run terraform plan first."))
        val review = item("i-2", ItemKind.REVIEW, listOf(InboxAction.ACCEPT, InboxAction.REPLY, InboxAction.REDO, InboxAction.LATER))
        assertEquals(InboxAction.ACCEPT, review.mainAnswer())
        assertEquals(InboxAction.ACCEPT, review.mainAnswer("  "))
        assertEquals(InboxAction.REPLY, review.mainAnswer("Also update the README."))
        val failure = item("i-3", ItemKind.FAILURE, listOf(InboxAction.RETRY, InboxAction.CHANGE_COURSE, InboxAction.LATER))
        assertEquals(InboxAction.RETRY, failure.mainAnswer())
        // A host without `reply` keeps its first answer, also with a text.
        assertEquals(InboxAction.RETRY, failure.mainAnswer("Try again."))
        // A question is answered by its options, so it has none.
        val asked = item("i-4", ItemKind.ASK, listOf(InboxAction.CHOOSE, InboxAction.REJECT, InboxAction.LATER), prompt = question)
        assertNull(asked.mainAnswer())
        assertEquals(listOf("TOML", "JSON"), asked.choices.map { it.label })
        assertTrue(ask.choices.isEmpty())
        // ADR 0041: with a text, Ask Agent when no other answer takes one.
        val briefing = item("e-1", ItemKind.BRIEFING, listOf(InboxAction.DISMISS, InboxAction.ASK_AGENT, InboxAction.LATER))
        assertEquals(InboxAction.DISMISS, briefing.mainAnswer())
        assertEquals(InboxAction.ASK_AGENT, briefing.mainAnswer("Put this in Notion."))
        val reviewAsks = item("i-5", ItemKind.REVIEW, listOf(InboxAction.ACCEPT, InboxAction.REPLY, InboxAction.ASK_AGENT))
        assertEquals(InboxAction.REPLY, reviewAsks.mainAnswer("Also update the README."))
        val olderReview = item("i-6", ItemKind.REVIEW, listOf(InboxAction.ACCEPT, InboxAction.REDO, InboxAction.ASK_AGENT))
        assertEquals(InboxAction.ACCEPT, olderReview.mainAnswer("Keep the old page."))
    }

    @Test
    fun aReplySendsItsTextAsItIs() = runBlocking {
        val bridge = bridge()
        val session = connected(bridge)
        session.answer(session.state.value.inboxItems[0], InboxAction.REPLY, text = "Do not apply.")
        val sent = bridge.asked.filterIsInstance<Request.UpdateInbox>().last()
        assertEquals(Request.UpdateInbox(sent.id, "i-00000001", InboxAction.REPLY, text = "Do not apply."), sent)
        assertTrue(InboxAction.REPLY.needsText)
    }

    @Test
    fun aMovedDownItemStillWaits() {
        assertEquals(true, item("i-3", ItemKind.REVIEW, listOf(InboxAction.ACCEPT)).copy(laterAt = 600).isWaiting(now = 400))
        // A host before ADR 0040 hid a Later item until its time.
        val later = item("i-1", ItemKind.REVIEW, listOf(InboxAction.ACCEPT), ItemState.LATER, until = 500)
        assertEquals(false, later.isWaiting(now = 400))
        assertEquals(true, later.isWaiting(now = 500))
        assertEquals(false, item("i-2", ItemKind.ASK, emptyList(), ItemState.RESOLVED).isWaiting(now = 0))
    }
}
