package tech.xdge.muxa.demo

import kotlinx.coroutines.asCoroutineDispatcher
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test
import tech.xdge.muxa.protocol.ErrorCode
import tech.xdge.muxa.protocol.InboxAction
import tech.xdge.muxa.protocol.ItemKind
import tech.xdge.muxa.protocol.ItemState
import tech.xdge.muxa.protocol.Message
import tech.xdge.muxa.protocol.Request
import tech.xdge.muxa.protocol.RunBudget
import tech.xdge.muxa.protocol.WORK_RESULT_REVIEW
import tech.xdge.muxa.protocol.WORK_TASK_PROPOSALS
import tech.xdge.muxa.protocol.WORK_TODAY_ACTIONS
import tech.xdge.muxa.protocol.Wire
import tech.xdge.muxa.session.PaneSession
import tech.xdge.muxa.session.isPlan
import tech.xdge.muxa.session.shown
import tech.xdge.muxa.session.startTaskAction
import java.util.concurrent.Executors

/** The demo's Today actions (ADR 0059, `work:17`) and proposed Tasks (ADR 0060, `work:18`), as the UI tests use them. */
class DemoProposalsTest {
    private suspend fun connected(level: Int = WORK_TASK_PROPOSALS, set: DemoBridge.Content = DemoBridge.Content.TESTS): PaneSession {
        val session = PaneSession(Executors.newSingleThreadExecutor().asCoroutineDispatcher())
        session.connect { DemoBridge(set, workLevel = level, assistPace = AssistPace.NEVER) }
        return session
    }

    private fun PaneSession.item(id: String) = state.value.inboxItems.first { it.id == id }

    @Test
    fun level18ListsProposalsPlansAndTheirTodayRow() = runBlocking {
        val s = connected().state.value
        val proposed = s.inboxItems.first { it.id == NEW_TASK }
        assertEquals(ItemKind.NEW_TASK to "app", proposed.kind to proposed.newTask?.repo)
        assertEquals(InboxAction.APPROVE, proposed.startTaskAction)
        assertEquals(InboxAction.START_TASK, s.inboxItems.first { it.id == CHAIN_ITEM }.startTaskAction)
        val plain = s.inboxItems.first { it.id == PLAIN_ITEM }
        assertEquals(InboxAction.APPROVE, plain.startTaskAction)
        assertTrue(plain.question.startsWith("The assistant proposes a Task: "))
        assertTrue(s.inboxItems.first { it.id == PLAN }.isPlan)
        assertTrue(s.plans.shown()!!.top.any { it.item == NEW_TASK })
        assertEquals(RunBudget(2, 3), s.overallRuns)
        assertEquals("secretary", s.secretary)
    }

    @Test
    fun level16HasNoneOfThem() = runBlocking {
        val s = connected(WORK_RESULT_REVIEW).state.value
        assertTrue(s.inboxItems.none { it.kind == ItemKind.NEW_TASK || it.isPlan })
        assertTrue(s.inboxItems.none { it.proposedTask() })
        assertNull(s.overallRuns)
        assertNull(s.secretary)
    }

    private fun tech.xdge.muxa.protocol.InboxItem.proposedTask() = result?.task != null

    @Test
    fun startTaskStartsOneTaskInItsWorkspace() = runBlocking {
        val session = connected()
        val saved = session.answer(session.item(NEW_TASK), InboxAction.APPROVE)
        val task = saved.task!!
        assertEquals(ItemState.ANSWERED, saved.state)
        assertTrue(saved.summary!!.startsWith("Started Task $task: Fix the flaky upload test"))
        val record = session.state.value.tasks!!.record(task)!!
        assertEquals("work" to "Fix the flaky upload test", record.workspace to record.title)
        assertTrue(record.goal.contains("Plan first"))
        assertTrue(session.state.value.sessions.any { it.taskId == task })
    }

    @Test
    fun startTaskBesideAHandoffEndsTheChain() = runBlocking {
        val session = connected()
        val saved = session.answer(session.item(CHAIN_ITEM), InboxAction.START_TASK)
        assertEquals(InboxAction.START_TASK, saved.answer)
        val record = session.state.value.tasks!!.record(saved.task!!)!!
        assertEquals("w-0000b005", record.work)
    }

    @Test
    fun sendOnAProposalOpensTheSecretaryAndKeepsItOpen() = runBlocking {
        val session = connected()
        val saved = session.answer(session.item(NEW_TASK), InboxAction.REPLY, text = "Use the api repo.")
        assertEquals(ItemState.OPEN, saved.state)
        assertTrue(session.state.value.sessions.any { it.name == "ask-secretary" })
    }

    @Test
    fun aStuckPlanSaysApproveInTheTerminal() = runBlocking {
        val session = connected()
        try {
            session.answer(session.item(STUCK_PLAN), InboxAction.APPROVE)
            fail("no option to press")
        } catch (e: PaneSession.Failure.ApproveInTerminal) {
            assertTrue(e.body.message.contains("terminal"))
        }
        assertEquals(InboxAction.APPROVE, session.answer(session.item(PLAN), InboxAction.APPROVE).answer)
    }

    @Test
    fun aWorkItemsSendOpensTheSecretaryAtLevel17() = runBlocking {
        val session = connected(WORK_TODAY_ACTIONS)
        val started = session.sendToSecretary("w-0000b001", "Ask the venues for May dates.")
        assertEquals("ask-secretary", started.name)
        try {
            session.sendToSecretary("w-ffffffff", "Hello.")
            fail("an unknown Work item")
        } catch (e: PaneSession.Failure.Bridge) {
            assertTrue(e.body.message.contains("w-ffffffff"))
        }
    }

    @Test
    fun proposeTaskReturnsTheOpenItemForTheSameProposal() = runBlocking {
        val bridge = DemoBridge(DemoBridge.Content.TESTS, workLevel = WORK_TASK_PROPOSALS, assistPace = AssistPace.NEVER)
        val first = bridge.call(Request.ProposeTask(1, "personal", title = "Collect the tax papers", goal = "List the papers still missing."))
        val again = bridge.call(Request.ProposeTask(2, "personal", title = "Collect the tax papers", goal = "List the papers still missing."))
        val made = first.filterIsInstance<Message.InboxSaved>().single().item!!
        assertEquals(ItemKind.NEW_TASK, made.kind)
        assertEquals(made.id, again.filterIsInstance<Message.InboxSaved>().single().item?.id)
        val refused = bridge.call(Request.ProposeTask(3, "nowhere", title = "T", goal = "G")).last() as Message.Result
        assertEquals(ErrorCode.BAD_REQUEST, refused.error?.code)
    }

    /** Sends [req] as the secretary's `muxa-cli task propose` does; returns what came until its `result`. */
    private suspend fun DemoBridge.call(req: Request): List<Message> {
        send(Wire.encode(req))
        val answered = mutableListOf<Message>()
        while (true) {
            val m = Wire.decodeMessage(lines.receive())
            answered += m
            if (m is Message.Result && m.id == req.id) return answered
        }
    }

    private companion object {
        const val NEW_TASK = "i-0000f301"
        const val CHAIN_ITEM = "i-0000f001"
        const val PLAIN_ITEM = "i-0000f002"
        const val PLAN = "i-0000f302"
        const val STUCK_PLAN = "i-0000f303"
    }
}
