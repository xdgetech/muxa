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
import tech.xdge.muxa.protocol.Handoff
import tech.xdge.muxa.protocol.InboxAction
import tech.xdge.muxa.protocol.InboxItem
import tech.xdge.muxa.protocol.ItemKind
import tech.xdge.muxa.protocol.ItemState
import tech.xdge.muxa.protocol.Message
import tech.xdge.muxa.protocol.Request
import tech.xdge.muxa.protocol.ResultReview
import tech.xdge.muxa.protocol.TaskProposal
import tech.xdge.muxa.protocol.TaskRecord
import tech.xdge.muxa.protocol.TaskStatus

/**
 * A host at `work` level 18 with a Result that proposes a Task, one that also offers a hand-off, the
 * secretary's `new-task` item and a plan item. Start Task makes the Task and answers the item as the host
 * does; [conflicts] answers `conflict` instead, as a plan menu without the manual option does.
 */
private class ProposalBridge : FakeBridge() {
    private val task = TaskProposal("muxa", "muxa", "Fix the Inbox crash", "Guard the empty summary.", step = 1, worker = "muxa-research")
    val result = InboxItem(
        "i-00000001", ItemKind.RESULT, ItemState.OPEN, "muxa-research proposes a Task: Fix the Inbox crash", "muxa", "Fix the crash", 9,
        result = ResultReview("q-00000001", 1, 1, "w-00000001", "muxa-research", task = task),
        answers = listOf(InboxAction.APPROVE, InboxAction.REPLY, InboxAction.DISMISS, InboxAction.LATER),
    )
    val besideHandoff = result.copy(
        id = "i-00000002",
        result = result.result!!.copy(handoff = Handoff("muxa-review", "Check the fix.")),
        answers = listOf(InboxAction.APPROVE, InboxAction.START_TASK, InboxAction.REPLY, InboxAction.DISMISS, InboxAction.LATER),
    )
    val proposed = InboxItem(
        "i-00000003", ItemKind.NEW_TASK, ItemState.OPEN, "The secretary proposes a Task in muxa: Fix the Inbox crash", "muxa",
        "Fix the Inbox crash", 9, summary = "Guard the empty summary.",
        newTask = TaskProposal("muxa", "muxa", "Fix the Inbox crash", "Guard the empty summary.", work = "w-00000001"),
        answers = listOf(InboxAction.APPROVE, InboxAction.REPLY, InboxAction.DISMISS, InboxAction.LATER),
    )
    val plan = InboxItem(
        "i-00000004", ItemKind.ASK, ItemState.OPEN, "Approve the plan?", "muxa", "Fix the Inbox crash", 9,
        task = "t-7f3a2c", session = "$1", pane = "%1",
        prompt = ChatPrompt("toolu_1", ChatPrompt.Kind.PLAN, options = listOf(ChatPrompt.Option("No, keep planning", listOf("Escape"), cancel = true))),
        answers = listOf(InboxAction.APPROVE, InboxAction.REJECT, InboxAction.REPLY, InboxAction.LATER),
    )
    var conflicts: ErrorBody? = null
    private val started = TaskRecord(
        "t-7f3a2c", 1, "muxa", "Fix the Inbox crash", "Guard the empty summary.", TaskStatus.OPEN, 10, 10, work = "w-00000001",
    )

    init {
        capabilities = listOf("task-records", "work:18")
    }

    override fun answer(req: Request) {
        when (req) {
            is Request.ListInbox -> {
                emit(Message.InboxList(req.id, emptyList(), items = listOf(result, besideHandoff, proposed, plan)))
                emit(Message.Result(req.id))
            }
            is Request.UpdateInbox -> {
                conflicts?.let { return emit(Message.Result(req.id, it)) }
                val item = listOf(result, besideHandoff, proposed, plan).first { it.id == req.entry }
                val starts = req.action == InboxAction.START_TASK || (req.action == InboxAction.APPROVE && item.kind != ItemKind.ASK)
                when {
                    starts -> {
                        tasks = tasks + started
                        emit(Message.TaskSaved(req.id, started))
                        val summary = "Started Task ${started.id}: ${started.title}"
                        emit(Message.InboxSaved(req.id, item = item.copy(state = ItemState.ANSWERED, answer = req.action, task = started.id, summary = summary, answers = emptyList())))
                    }
                    item.kind == ItemKind.NEW_TASK && req.action == InboxAction.REPLY -> {
                        emit(Message.SessionStarted(req.id, "$8", "ask-secretary"))
                        emit(Message.InboxSaved(req.id, item = item))
                    }
                    else -> emit(Message.InboxSaved(req.id, item = item.copy(state = ItemState.ANSWERED, answer = req.action, answers = emptyList())))
                }
                emit(Message.Result(req.id))
            }
            else -> super.answer(req)
        }
    }
}

class TaskProposalTest {
    private suspend fun connected(bridge: ProposalBridge): PaneSession {
        val session = PaneSession(oneThread())
        session.connect {
            bridge.start()
            bridge
        }
        session.loadWork()
        return session
    }

    private fun ProposalBridge.answers() = received.filterIsInstance<Request.UpdateInbox>()

    @Test
    fun startTaskIsApproveAloneAndStartTaskBesideAHandoff() {
        val bridge = ProposalBridge()
        assertEquals(InboxAction.APPROVE, bridge.result.startTaskAction)
        assertEquals(InboxAction.START_TASK, bridge.besideHandoff.startTaskAction)
        assertEquals(InboxAction.APPROVE, bridge.proposed.startTaskAction)
        assertEquals("the proposal is the item's", "w-00000001", bridge.proposed.proposedTask?.work)
        // A level 17 host lists no start-task: its approve hands off, and nothing here starts the Task.
        assertNull(bridge.besideHandoff.copy(answers = bridge.besideHandoff.answers - InboxAction.START_TASK).startTaskAction)
        assertNull(bridge.plan.startTaskAction)
        // Hand off and Start Task are both decisions; Start Task is the main one.
        assertEquals(listOf(InboxAction.APPROVE, InboxAction.START_TASK), bridge.besideHandoff.buttons.decisions)
        assertEquals(InboxAction.START_TASK, bridge.besideHandoff.mainAnswer())
        assertEquals(InboxAction.APPROVE, bridge.proposed.mainAnswer())
        assertEquals(InboxAction.REPLY, bridge.proposed.mainAnswer("Use the muxa-cli repo."))
    }

    @Test
    fun startTaskAnswersTheItemWithTheNewTask() = runBlocking {
        val bridge = ProposalBridge()
        val session = connected(bridge)
        val saved = session.answer(bridge.besideHandoff, InboxAction.START_TASK)
        assertEquals(InboxAction.START_TASK, bridge.answers().single().action)
        assertEquals(Triple(ItemState.ANSWERED, InboxAction.START_TASK, "t-7f3a2c"), Triple(saved.state, saved.answer, saved.task))
        assertEquals("t-7f3a2c", saved.startedTask)
        // The Tasks list has the new Task.
        assertTrue(session.state.value.tasks?.records.orEmpty().any { it.id == "t-7f3a2c" })
    }

    @Test
    fun sendOnAProposedTaskKeepsItOpen() = runBlocking {
        val bridge = ProposalBridge()
        val session = connected(bridge)
        val saved = session.answer(bridge.proposed, InboxAction.REPLY, text = "Use the muxa-cli repo instead.")
        val sent = bridge.answers().single()
        assertEquals(InboxAction.REPLY to "Use the muxa-cli repo instead.", sent.action to sent.text)
        assertEquals(ItemState.OPEN, saved.state)
        assertNull(saved.startedTask)
    }

    @Test
    fun aStartRefusedIsTheHostsReason() = runBlocking {
        val bridge = ProposalBridge()
        val session = connected(bridge)
        bridge.conflicts = ErrorBody(ErrorCode.CONFLICT, "workspace \"muxa\" has 3 open Tasks, its max_open_tasks")
        try {
            session.answer(bridge.proposed, InboxAction.APPROVE)
            fail("a limit refuses Start Task")
        } catch (e: PaneSession.Failure.Refused) {
            assertTrue(PaneSession.describe(e).contains("max_open_tasks"))
        }
    }

    @Test
    fun approvePlanThatFindsNoOptionSaysApproveInTheTerminal() = runBlocking {
        val bridge = ProposalBridge()
        val session = connected(bridge)
        assertTrue(bridge.plan.isPlan)
        bridge.conflicts = ErrorBody(ErrorCode.CONFLICT, "the plan menu does not show one option; approve the plan in the Task's terminal")
        try {
            session.answer(bridge.plan, InboxAction.APPROVE)
            fail("no option to press")
        } catch (e: PaneSession.Failure.ApproveInTerminal) {
            assertEquals(PaneSession.APPROVE_IN_TERMINAL, PaneSession.describe(e))
        }
        // Keep planning's conflict is a changed menu, as on any ask.
        try {
            session.answer(bridge.plan, InboxAction.REJECT)
            fail("the menu changed")
        } catch (e: PaneSession.Failure.ItemChanged) {
            assertEquals(PaneSession.ITEM_CHANGED, PaneSession.describe(e))
        }
    }

    @Test
    fun keepPlanningAndSendIsAReply() = runBlocking {
        val bridge = ProposalBridge()
        val session = connected(bridge)
        session.answer(bridge.plan, InboxAction.REPLY, text = "Add a test first.")
        val sent = bridge.answers().single()
        assertEquals(InboxAction.REPLY to "Add a test first.", sent.action to sent.text)
    }
}
