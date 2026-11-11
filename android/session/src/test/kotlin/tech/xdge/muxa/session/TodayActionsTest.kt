package tech.xdge.muxa.session

import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test
import tech.xdge.muxa.protocol.AssistantInfo
import tech.xdge.muxa.protocol.ChatPrompt
import tech.xdge.muxa.protocol.ErrorBody
import tech.xdge.muxa.protocol.ErrorCode
import tech.xdge.muxa.protocol.InboxAction
import tech.xdge.muxa.protocol.InboxItem
import tech.xdge.muxa.protocol.ItemKind
import tech.xdge.muxa.protocol.ItemState
import tech.xdge.muxa.protocol.Message
import tech.xdge.muxa.protocol.Minutes
import tech.xdge.muxa.protocol.Pane
import tech.xdge.muxa.protocol.PlanRow
import tech.xdge.muxa.protocol.Request
import tech.xdge.muxa.protocol.ResultReview
import tech.xdge.muxa.protocol.Role
import tech.xdge.muxa.protocol.RunBudget
import tech.xdge.muxa.protocol.Session
import tech.xdge.muxa.protocol.WriteProposal

/** Today rows say what to do (ADR 0059): the verbs, the primary action, who gets Send, and the secretary's Send. */
class TodayActionsTest {
    /** A host at [level] with the secretary as its overall workspace; [refusal] answers `start-session` instead. */
    private class SecretaryBridge(level: Int) : FakeBridge() {
        var refusal: ErrorBody? = null

        init {
            capabilities = listOf("task-records", "work:$level")
        }

        override fun answer(req: Request) {
            when {
                req is Request.ListInbox -> {
                    val runs = RunBudget(2, 3).takeIf { capabilities.contains("work:17") }
                    emit(Message.InboxList(req.id, emptyList(), assistant = AssistantInfo("secretary", overallRuns = runs)))
                    emit(Message.Result(req.id))
                }
                req is Request.StartSession && req.root -> {
                    refusal?.let { return emit(Message.Result(req.id, it)) }
                    sessions = sessions + Session(
                        id = "$8", name = "ask-secretary", ask = req.workspace,
                        panes = listOf(Pane("%8", "@8", 0, Role.AGENT, 80, 24, "claude", "/secretary", false, false)),
                    )
                    emit(Message.SessionStarted(req.id, "$8", "ask-secretary"))
                    emit(Message.Result(req.id))
                }
                else -> super.answer(req)
            }
        }
    }

    private suspend fun connected(bridge: FakeBridge): PaneSession {
        val session = PaneSession(oneThread())
        session.connect {
            bridge.start()
            bridge
        }
        session.loadWork()
        return session
    }

    private fun item(kind: ItemKind, question: String = "q", answers: List<InboxAction> = emptyList()) =
        InboxItem("i-1", kind, ItemState.OPEN, question, "work", "t", 1, answers = answers)

    private val row = PlanRow(item = "i-1", why = "w", next = "n", minutes = Minutes(1, 2, 1))

    @Test
    fun aRowNamesItsActionFromTheTargetsKind() {
        assertEquals("Start", rowVerb(row.copy(item = null, work = "w-1"), null))
        assertEquals("Open", rowVerb(row, null))
        val verbs = mapOf(
            ItemKind.REVIEW to "Review changes", ItemKind.ASK to "Allow or deny", ItemKind.RESULT to "Read result",
            ItemKind.WRITE to "Check text to send", ItemKind.FAILURE to "Fix", ItemKind.MEMORY to "Check memory note",
            ItemKind.PROPOSAL to "Approve or reject", ItemKind.SORT to "Sort capture", ItemKind.DELEGATE to "Approve request",
            ItemKind.HYPOTHESIS to "Confirm or reject", ItemKind.HANDOFF to "Approve hand-off", ItemKind.NEW_TASK to "Start Task",
            ItemKind("later-kind") to "Open",
        )
        verbs.forEach { (kind, verb) -> assertEquals(kind.raw, verb, rowVerb(row, item(kind))) }
        // A briefing asks to decide only when its question ends in "?".
        assertEquals("Decide", rowVerb(row, item(ItemKind.BRIEFING, "Book the venue? ")))
        assertEquals("Read", rowVerb(row, item(ItemKind.BRIEFING, "The week in review")))
    }

    @Test
    fun theMainActionOpensATasksSessionOrTheTarget() {
        assertEquals("Open the Task (review)", primaryLabel(item(ItemKind.REVIEW).copy(session = "$1")))
        assertEquals("Open the Task (allow or deny)", primaryLabel(item(ItemKind.ASK).copy(session = "$1")))
        assertEquals("a review without a session moves to it", "Open the item", primaryLabel(item(ItemKind.REVIEW)))
        assertEquals("Open the result", primaryLabel(item(ItemKind.RESULT)))
        assertEquals("Open the proposed Task", primaryLabel(item(ItemKind.NEW_TASK)))
        assertEquals("Open the Work item", primaryLabel(null))
    }

    @Test
    fun sendNamesWhoGetsTheText() {
        val reply = listOf(InboxAction.REPLY)
        fun label(i: InboxItem) = i.todaySend?.label
        assertEquals("Send to the Task's agent…", label(item(ItemKind.REVIEW, answers = reply)))
        assertEquals("Deny & send to the Task's agent…", label(item(ItemKind.ASK, answers = reply)))
        val plan = item(ItemKind.ASK, answers = reply).copy(prompt = ChatPrompt("e", ChatPrompt.Kind.PLAN, options = emptyList()))
        assertEquals("Keep planning & send to the Task's agent…", label(plan))
        val result = item(ItemKind.RESULT, answers = reply).copy(result = ResultReview("q-1", 1, 1, "w-1", "work-writer"))
        assertEquals("Send to work-writer…", label(result))
        assertEquals("Send to the assistant…", label(result.copy(result = result.result!!.copy(worker = null))))
        val write = item(ItemKind.WRITE, answers = reply).copy(write = WriteProposal("q-1", 1, 1, "work-writer", "a", "t", "c", "h"))
        assertEquals("Revise: send to work-writer…", label(write))
        assertEquals("Send to the secretary…", label(item(ItemKind.NEW_TASK, answers = reply)))
        val briefing = item(ItemKind.BRIEFING, answers = listOf(InboxAction.ASK_AGENT))
        assertEquals("Send to the work Ask session, with this briefing…", label(briefing))
        assertTrue(briefing.todaySend is TodaySend.AskAgent)
        assertNull("neither answer: nothing to send", item(ItemKind.SORT, answers = listOf(InboxAction.APPROVE)).todaySend)
        assertEquals("Send to the secretary (secretary), with this Work item…", workSend("w-1", "secretary")?.label)
        assertNull(workSend("w-1", null))
        assertEquals("2 of 3 runs left today", RunBudget(2, 3).label)
    }

    @Test
    fun aWorkItemsSendTypesIntoTheSecretaryAtLevel17() = runBlocking {
        val bridge = SecretaryBridge(17)
        val session = connected(bridge)
        assertEquals("secretary", session.state.value.secretary)
        assertEquals(RunBudget(2, 3), session.state.value.overallRuns)
        val started = session.sendToSecretary("w-7b8c9d0e", "Ask legal whether offer B fits.")
        assertEquals("ask-secretary", started.name)
        val sent = bridge.received.filterIsInstance<Request.StartSession>().single()
        assertEquals(listOf("secretary", "w-7b8c9d0e", "Ask legal whether offer B fits."), listOf(sent.workspace, sent.work, sent.text))
        assertTrue(sent.root && sent.repo == null)
        assertTrue(session.state.value.sessions.any { it.id == "$8" })
    }

    @Test
    fun aBusySecretaryIsTheHostsReason() = runBlocking {
        val bridge = SecretaryBridge(17)
        val session = connected(bridge)
        bridge.refusal = ErrorBody(ErrorCode.CONFLICT, "the agent waits on a menu; nothing was typed")
        try {
            session.sendToSecretary("w-1", "Go on.")
            fail("a menu refuses Send")
        } catch (e: PaneSession.Failure.Bridge) {
            assertEquals(ErrorCode.CONFLICT, e.body.code)
        }
    }

    /** A level 16 host would open the session and drop the text: the client never sends it there. */
    @Test
    fun aLevel16HostGetsNoText() = runBlocking {
        val bridge = SecretaryBridge(16)
        val session = connected(bridge)
        assertNull(session.state.value.secretary)
        assertNull(session.state.value.overallRuns)
        try {
            session.sendToSecretary("w-1", "Go on.")
            fail("a level 16 host cannot")
        } catch (e: PaneSession.Failure.Bridge) {
            assertEquals(PaneSession.SECRETARY_UNSUPPORTED, PaneSession.describe(e))
        }
        assertTrue(bridge.received.none { it is Request.StartSession })
    }
}
