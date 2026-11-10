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
import tech.xdge.muxa.protocol.AssistSpec
import tech.xdge.muxa.protocol.AssistantInfo
import tech.xdge.muxa.protocol.AssistantRequest
import tech.xdge.muxa.protocol.DelegateInfo
import tech.xdge.muxa.protocol.DelegateProposal
import tech.xdge.muxa.protocol.Delivery
import tech.xdge.muxa.protocol.ErrorBody
import tech.xdge.muxa.protocol.ErrorCode
import tech.xdge.muxa.protocol.Handoff
import tech.xdge.muxa.protocol.HandoffProposal
import tech.xdge.muxa.protocol.InboxAction
import tech.xdge.muxa.protocol.InboxItem
import tech.xdge.muxa.protocol.ItemKind
import tech.xdge.muxa.protocol.ItemState
import tech.xdge.muxa.protocol.Message
import tech.xdge.muxa.protocol.Notes
import tech.xdge.muxa.protocol.OverallRequests
import tech.xdge.muxa.protocol.Request
import tech.xdge.muxa.protocol.RequestStep
import tech.xdge.muxa.protocol.Share
import tech.xdge.muxa.protocol.StaffMember
import tech.xdge.muxa.protocol.StepBy
import tech.xdge.muxa.protocol.WorkItem
import tech.xdge.muxa.protocol.WorkState

/**
 * A host at [level] with one Work item whose request ran two steps and waits on a hand-off. [refusal]
 * answers the next `update-inbox`, as a budget's `conflict` does.
 */
private class StaffBridge(level: Int) : FakeBridge() {
    val item = WorkItem("w-00000001", 3, "Pick a tracker", WorkState.OPEN, 1, 1, "work", requests = listOf("q-00000001"))
    var request = chained()
    val handoff = InboxItem(
        "i-00000001", ItemKind.HANDOFF, ItemState.OPEN, "work-writer asks work-review to: check it", "work", "Pick a tracker", 9,
        handoff = HandoffProposal("q-00000001", 1, 2, item.id, "work-writer", "work-review", "check it"),
        answers = listOf(InboxAction.APPROVE, InboxAction.REJECT, InboxAction.LATER),
    )
    var refusal: ErrorBody? = null

    init {
        capabilities = listOf("task-records", "work:$level")
    }

    override fun answer(req: Request) {
        when (req) {
            is Request.ListInbox -> {
                emit(Message.InboxList(req.id, emptyList(), items = listOf(handoff), work = listOf(item), requests = listOf(request), assistant = INFO))
                emit(Message.Result(req.id))
            }
            is Request.RequestAssistant -> {
                emit(Message.WorkSaved(req.id, item, request))
                emit(Message.Result(req.id))
            }
            is Request.Capture -> {
                emit(Message.Result(req.id, ErrorBody(ErrorCode.INTERNAL, "not in this test")))
            }
            is Request.ApplyNotes -> {
                emit(Message.WorkSaved(req.id, item, request))
                emit(Message.Result(req.id))
            }
            is Request.UpdateInbox -> {
                refusal?.let { return emit(Message.Result(req.id, it)) }
                request = request.copy(steps = request.steps + RequestStep(3, "work-review", StepBy.HANDOFF, Delivery.PENDING, 10, "t-run3"))
                emit(Message.WorkSaved(req.id, item, request))
                emit(Message.InboxSaved(req.id, item = handoff.copy(state = ItemState.ANSWERED, answer = req.action, answers = emptyList())))
                emit(Message.Result(req.id))
            }
            else -> super.answer(req)
        }
    }

    companion object {
        val STAFF = listOf(
            StaffMember("work-research", "Reads and reports.", lead = true),
            StaffMember("work-writer", "Drafts."),
            StaffMember("work-review", "Checks drafts."),
        )
        val INFO = AssistantInfo(
            "secretary",
            listOf(
                DelegateInfo("work", Share.SUMMARIES, OverallRequests.REVIEW, staff = STAFF),
                DelegateInfo("personal", Share.OUTLINE, OverallRequests.REVIEW, staff = listOf(StaffMember("home-planner", "Plans."))),
            ),
        )

        fun chained() = AssistantRequest(
            "q-00000001", "w-00000001", 1, "work", AssistScope.RESEARCH, Delivery.RESULTED, 5, 1, 5,
            results = listOf(
                AssistResult(1, 5, "The table.", worker = "work-writer", step = 2, notes = Notes("# writer", "e3b0")),
                AssistResult(1, 3, "Two fit.", worker = "work-research", step = 1),
            ),
            steps = listOf(
                RequestStep(1, "work-research", StepBy.LEAD, Delivery.RESULTED, 3, "t-run1", Handoff("work-writer", "draft it")),
                RequestStep(2, "work-writer", StepBy.HANDOFF, Delivery.RESULTED, 5, "t-run2", Handoff("work-review", "check it")),
            ),
        )
    }
}

class StaffTest {
    private suspend fun connected(bridge: StaffBridge): PaneSession {
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

    @Test
    fun aNamedMemberIsSentAtLevel12() = runBlocking {
        val bridge = StaffBridge(12)
        val session = connected(bridge)
        assertTrue(session.state.value.canAskStaff)
        session.requestAssistant(bridge.item, AssistScope.RESEARCH, worker = "work-writer")
        session.requestAssistant(bridge.item, AssistScope.RESEARCH)
        assertEquals(listOf("work-writer", null), bridge.received.filterIsInstance<Request.RequestAssistant>().map { it.worker })
    }

    @Test
    fun aLevel11HostGetsNoWorkerAndNoStep() = runBlocking {
        val bridge = StaffBridge(11)
        val session = connected(bridge)
        assertFalse(session.state.value.canAskStaff)
        val asked = refusal { session.requestAssistant(bridge.item, AssistScope.RESEARCH, worker = "work-writer") }
        assertEquals(PaneSession.STAFF_UNSUPPORTED, PaneSession.describe(asked))
        refusal { session.captureAndAsk("c-1", "Idea", "work", AssistSpec(AssistScope.RESEARCH, worker = "work-writer")) }
        refusal { session.applyNotes("q-00000001", 1, Notes("# writer", "e3b0"), step = 2) }
        val sent = bridge.received
        assertTrue(sent.none { it is Request.RequestAssistant || it is Request.Capture || it is Request.ApplyNotes })
        // Without a worker, a level 11 host is asked as before.
        session.requestAssistant(bridge.item, AssistScope.RESEARCH)
        assertNull(bridge.received.filterIsInstance<Request.RequestAssistant>().single().worker)
    }

    @Test
    fun applyNotesSendsTheStep() = runBlocking {
        val bridge = StaffBridge(12)
        val session = connected(bridge)
        session.applyNotes("q-00000001", 1, Notes("# writer", "e3b0"), step = 2)
        session.applyNotes("q-00000001", 1, Notes("# lead", "e3b0"))
        assertEquals(listOf(2, null), bridge.received.filterIsInstance<Request.ApplyNotes>().map { it.step })
    }

    @Test
    fun approvingAHandoffListsTheWorkAgainAndABudgetIsTheHostsReason() = runBlocking {
        val bridge = StaffBridge(12)
        val session = connected(bridge)
        val item = session.state.value.inboxItems.single()
        session.answer(item, InboxAction.APPROVE)
        assertEquals(3, session.state.value.requests.single().steps.size)
        bridge.refusal = ErrorBody(ErrorCode.CONFLICT, "max_open_runs of work reached")
        val e = refusal { session.answer(item, InboxAction.APPROVE) }
        assertTrue(e is PaneSession.Failure.Refused)
        assertEquals("max_open_runs of work reached", PaneSession.describe(e))
    }

    @Test
    fun theChainPairsEachStepWithItsResult() {
        val q = StaffBridge.chained()
        assertEquals("work-research → work-writer", q.chainLine)
        assertEquals(listOf("Two fit.", "The table."), q.chain.map { it.result?.summary })
        assertEquals(2, q.askedHandoff?.step)
        // A Result without `step` is step 1; an older version's is not the current step's.
        val plain = q.copy(version = 2, steps = q.steps.take(1), results = listOf(AssistResult(2, 9, "New."), AssistResult(1, 3, "Old.", stale = true)))
        assertEquals("New.", plain.resultOf(1)?.summary)
        assertNull(plain.resultOf(2))
    }

    @Test
    fun aHandoffWaitsWhileItsItemIsOpenAndEndsOnReject() {
        val bridge = StaffBridge(12)
        val q = bridge.request
        val step = q.steps.last()
        val open = q.handoffItem(step, listOf(bridge.handoff))
        assertEquals(HandoffState.WAITING, handoffState(open))
        assertEquals(listOf(InboxAction.APPROVE, InboxAction.REJECT), open?.handoffAnswers)
        val rejected = bridge.handoff.copy(state = ItemState.ANSWERED, answer = InboxAction.REJECT)
        assertEquals(HandoffState.ENDED, handoffState(rejected))
        assertEquals(HandoffState.ENDED, handoffState(bridge.handoff.copy(state = ItemState.RESOLVED)))
        assertNull(handoffState(bridge.handoff.copy(state = ItemState.ANSWERED, answer = InboxAction.APPROVE)))
        assertNull(handoffState(null))
        // An item about another version or step is not this step's.
        assertNull(q.handoffItem(q.steps.first(), listOf(bridge.handoff)))
        assertNull(q.copy(version = 2).handoffItem(step, listOf(bridge.handoff)))
    }

    @Test
    fun thePickerOffersAChoiceOnlyWithMoreThanOneMember() {
        val work = StaffBridge.INFO.staffOf("work")
        assertTrue(work.offersChoice)
        assertEquals("Let work-research decide", leadChoice(work))
        val personal = StaffBridge.INFO.staffOf("personal")
        assertFalse(personal.offersChoice)
        assertEquals("home-planner", personal.lead?.worker)
        assertTrue(StaffBridge.INFO.staffOf("notes").isEmpty())
        assertTrue((null as AssistantInfo?).staffOf("work").isEmpty())
        // Without a member marked lead, the first is the lead.
        assertEquals("work-writer", StaffBridge.STAFF.drop(1).lead?.worker)
    }

    @Test
    fun theLeadsNotesAreTheWorkspacesAndAMembersAreItsOwn() {
        assertEquals(NOTES_FILE, notesFile("work-research", StaffBridge.STAFF))
        assertEquals(".muxa/assistant/staff/work-writer/NOTES.md", notesFile("work-writer", StaffBridge.STAFF))
        assertEquals(NOTES_FILE, notesFile(null, emptyList()))
    }

    @Test
    fun aDelegateNamesItsMemberAndUnknownPickersReadAsUnknown() {
        val d = DelegateProposal("w-1", "work", AssistScope.RESEARCH, "p", "r")
        assertEquals(delegateName("work"), d.addressee)
        assertEquals("work-review in work", d.copy(worker = "work-review").addressee)
        assertEquals("unknown", StepBy("someone").label)
        assertEquals("handed on", StepBy.HANDOFF.label)
    }
}
