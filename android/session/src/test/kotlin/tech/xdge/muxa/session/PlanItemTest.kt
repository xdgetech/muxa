package tech.xdge.muxa.session

import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
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

/** A host with Claude Code's plan menu as an `ask` item. [conflicts] answers `conflict`, as a menu without the manual option does. */
private class PlanBridge : FakeBridge() {
    val plan = InboxItem(
        "i-00000004", ItemKind.ASK, ItemState.OPEN, "Approve the plan?", "muxa", "Fix the Inbox crash", 9,
        task = "t-7f3a2c", session = "$1", pane = "%1",
        prompt = ChatPrompt("toolu_1", ChatPrompt.Kind.PLAN, options = listOf(ChatPrompt.Option("No, keep planning", listOf("Escape"), cancel = true))),
        answers = listOf(InboxAction.APPROVE, InboxAction.REJECT, InboxAction.REPLY, InboxAction.LATER),
    )
    var conflicts: ErrorBody? = null

    init {
        capabilities = listOf("task-records", "work:3")
    }

    override fun answer(req: Request) {
        when (req) {
            is Request.ListInbox -> {
                emit(Message.InboxList(req.id, emptyList(), items = listOf(plan)))
                emit(Message.Result(req.id))
            }
            is Request.UpdateInbox -> {
                conflicts?.let { return emit(Message.Result(req.id, it)) }
                emit(Message.InboxSaved(req.id, item = plan.copy(state = ItemState.ANSWERED, answer = req.action, answers = emptyList())))
                emit(Message.Result(req.id))
            }
            else -> super.answer(req)
        }
    }
}

/** Approve plan and Keep planning on an `ask` item (ADR 0060 §5). */
class PlanItemTest {
    private suspend fun connected(bridge: PlanBridge): PaneSession {
        val session = PaneSession(oneThread())
        session.connect {
            bridge.start()
            bridge
        }
        session.loadWork()
        return session
    }

    @Test
    fun approvePlanThatFindsNoOptionSaysApproveInTheTerminal() = runBlocking {
        val bridge = PlanBridge()
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
        val bridge = PlanBridge()
        val session = connected(bridge)
        session.answer(bridge.plan, InboxAction.REPLY, text = "Add a test first.")
        val sent = bridge.received.filterIsInstance<Request.UpdateInbox>().single()
        assertEquals(InboxAction.REPLY to "Add a test first.", sent.action to sent.text)
    }
}
