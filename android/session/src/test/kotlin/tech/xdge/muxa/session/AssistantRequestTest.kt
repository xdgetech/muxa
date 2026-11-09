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
import tech.xdge.muxa.protocol.AssistantRequest
import tech.xdge.muxa.protocol.Capture
import tech.xdge.muxa.protocol.CaptureSource
import tech.xdge.muxa.protocol.CaptureState
import tech.xdge.muxa.protocol.Delivery
import tech.xdge.muxa.protocol.ErrorBody
import tech.xdge.muxa.protocol.ErrorCode
import tech.xdge.muxa.protocol.Message
import tech.xdge.muxa.protocol.Request
import tech.xdge.muxa.protocol.WorkItem
import tech.xdge.muxa.protocol.WorkState

/**
 * A host at `work` level 8 with one Work item, its Capture and one finished request. [refusal] answers
 * the next `request-assistant` or assisted `capture` after the Capture is saved, as the host does.
 */
private class AssistBridge(level: Int = 8) : FakeBridge() {
    val text = "Plan the offsite\nTwo days in May."
    var capture = Capture("c-00000001", 2, "x", text, CaptureSource.APP, CaptureState.SORTED, 1, 1, "work", listOf("w-00000001"))
    var item = WorkItem("w-00000001", 3, "Plan the offsite", WorkState.OPEN, 1, 1, "work", captures = listOf(capture.id), requests = listOf("q-00000001"))
    var request = AssistantRequest(
        "q-00000001", item.id, 1, "work", AssistScope.RESEARCH, Delivery.RESULTED, 5, 1, 5,
        results = listOf(AssistResult(1, 5, "Two venues fit.")),
    )
    var refusal: ErrorBody? = null

    init {
        capabilities = listOf("task-records", "work:$level")
    }

    override fun answer(req: Request) {
        when (req) {
            is Request.ListInbox -> {
                emit(Message.InboxList(req.id, emptyList(), work = listOf(item), requests = listOf(request)))
                emit(Message.Result(req.id))
            }
            is Request.GetWork -> {
                if (req.work != item.id) return emit(Message.Result(req.id, ErrorBody(ErrorCode.BAD_REQUEST, "no Work item ${req.work}")))
                emit(Message.WorkDetail(req.id, item, listOf(capture), listOf(request)))
                emit(Message.Result(req.id))
            }
            is Request.RequestAssistant -> {
                if (req.rev != item.rev) return emit(Message.Result(req.id, ErrorBody(ErrorCode.CONFLICT, "Work item changed")))
                refusal?.let { return emit(Message.Result(req.id, it)) }
                request = request.copy(version = request.version + 1, scope = req.scope, purpose = req.purpose, delivery = Delivery.PENDING, run = "t-run")
                item = item.copy(rev = item.rev + 1)
                emit(Message.WorkSaved(req.id, item, request))
                emit(Message.Result(req.id))
            }
            is Request.CancelRequest -> {
                if (req.version != request.version) {
                    return emit(Message.Result(req.id, ErrorBody(ErrorCode.CONFLICT, "request q-00000001 is at version ${request.version}; list it again")))
                }
                request = request.copy(delivery = Delivery.CANCEL_REQUESTED)
                emit(Message.WorkSaved(req.id, item, request))
                emit(Message.Result(req.id))
            }
            is Request.Capture -> {
                val saved = Capture("c-00000002", 1, req.clientId, req.text, CaptureSource.APP, CaptureState.RECEIVED, 6, 6, req.workspace)
                emit(Message.CaptureSaved(req.id, saved))
                emit(Message.Result(req.id, refusal))
            }
            else -> super.answer(req)
        }
    }
}

class AssistantRequestTest {
    private suspend fun connected(bridge: AssistBridge): PaneSession {
        val session = PaneSession(oneThread())
        session.connect {
            bridge.start()
            bridge
        }
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
    fun theInboxListKeepsTheRequestsAndTheRowSaysResultReady() = runBlocking {
        val s = connected(AssistBridge()).state.value
        assertEquals(listOf("q-00000001"), s.requests.map { it.id })
        assertEquals(RESULT_READY, assistNote(s.work.single(), s.requests))
    }

    @Test
    fun getWorkKeepsEveryCaptureTextByItem() = runBlocking {
        val bridge = AssistBridge()
        val session = connected(bridge)
        val detail = session.getWork("w-00000001")
        assertEquals(listOf(bridge.text), detail.captures.map { it.text })
        assertEquals(detail, session.state.value.workDetails["w-00000001"])
        val unknown = refusal { session.getWork("w-0000ffff") }
        assertEquals("no Work item w-0000ffff", PaneSession.describe(unknown))
    }

    @Test
    fun askAgainSendsTheRevScopeAndPurposeAndListsTheInboxAgain() = runBlocking {
        val bridge = AssistBridge()
        val session = connected(bridge)
        val item = session.state.value.work.single()
        val q = session.requestAssistant(item, AssistScope.COMPARE, "Which venue is cheaper")
        val sent = bridge.received.filterIsInstance<Request.RequestAssistant>().single()
        assertEquals(Request.RequestAssistant(sent.id, item.id, 3, AssistScope.COMPARE, null, "Which venue is cheaper"), sent)
        assertEquals(2L, q.version)
        assertEquals(Delivery.PENDING, session.state.value.requests.single().delivery)
        assertEquals(4L, session.state.value.work.single().rev)
    }

    @Test
    fun aStaleRevIsWorkChanged() = runBlocking {
        val bridge = AssistBridge()
        val session = connected(bridge)
        val seen = session.state.value.work.single()
        bridge.item = bridge.item.copy(rev = 9)
        val e = refusal { session.requestAssistant(seen, AssistScope.RESEARCH) }
        assertTrue(e is PaneSession.Failure.WorkChanged)
    }

    /** A limit is `conflict` too; the user sees the host's message, with when it resets. */
    @Test
    fun aLimitShowsTheHostsMessage() = runBlocking {
        val bridge = AssistBridge()
        val session = connected(bridge)
        val limit = "workspace \"work\" started 10 assistant runs today, its max_runs_per_day. The count resets at 00:00."
        bridge.refusal = ErrorBody(ErrorCode.CONFLICT, limit)
        val e = refusal { session.requestAssistant(session.state.value.work.single(), AssistScope.RESEARCH) }
        assertTrue(e is PaneSession.Failure.Refused)
        assertEquals(limit, PaneSession.describe(e))
    }

    @Test
    fun noDaemonShowsTheHostsMessage() = runBlocking {
        val bridge = AssistBridge()
        val session = connected(bridge)
        val off = "Automatic processing is off: assistant requests run in the muxa daemon."
        bridge.refusal = ErrorBody(ErrorCode.UNSUPPORTED, off)
        val e = refusal { session.requestAssistant(session.state.value.work.single(), AssistScope.RESEARCH) }
        assertEquals(off, PaneSession.describe(e))
    }

    @Test
    fun cancelSendsTheVersionSeenAndANewerOneShowsTheHostsMessage() = runBlocking {
        val bridge = AssistBridge()
        val session = connected(bridge)
        session.requestAssistant(session.state.value.work.single(), AssistScope.RESEARCH)
        val running = session.state.value.requests.single()
        assertTrue(running.canCancel)
        assertEquals(Delivery.CANCEL_REQUESTED, session.cancelRequest(running).delivery)
        assertEquals(2L, bridge.received.filterIsInstance<Request.CancelRequest>().single().version)
        val e = refusal { session.cancelRequest(running.copy(version = 1)) }
        assertEquals("request q-00000001 is at version 2; list it again", PaneSession.describe(e))
    }

    /** The host keeps the Capture when the assistant cannot start, and says why. */
    @Test
    fun anAssistedCaptureIsKeptWhenTheAssistantIsRefused() = runBlocking {
        val bridge = AssistBridge()
        val session = connected(bridge)
        bridge.refusal = ErrorBody(ErrorCode.UNSUPPORTED, "Automatic processing is off.")
        val a = session.captureAndAsk("client-1", "Compare venues", "work", AssistSpec(AssistScope.COMPARE))
        assertEquals("c-00000002", a.capture.id)
        assertEquals("Automatic processing is off.", a.refusal?.let(PaneSession::describe))
        val sent = bridge.received.filterIsInstance<Request.Capture>().single()
        assertEquals(AssistSpec(AssistScope.COMPARE), sent.assist)
        assertEquals("work", sent.workspace)
    }

    @Test
    fun anAssistedCaptureWithoutARefusalHasNone() = runBlocking {
        val session = connected(AssistBridge())
        assertNull(session.captureAndAsk("client-2", "Idea", "work", AssistSpec(AssistScope.RESEARCH)).refusal)
    }

    @Test
    fun anOlderHostIsNeverAskedForWorkOrRequests() = runBlocking {
        val bridge = AssistBridge(level = 7)
        val session = connected(bridge)
        assertTrue(session.state.value.requests.isEmpty())
        refusal { session.getWork("w-00000001") }
        refusal { session.requestAssistant(bridge.item, AssistScope.RESEARCH) }
        assertTrue(bridge.received.none { it is Request.GetWork || it is Request.RequestAssistant })
    }

    @Test
    fun deliveryWordsAndActions() {
        val q = AssistantRequest("q-1", "w-1", 2, "work", AssistScope.DRAFT, Delivery.PROCESSING, 1, 1, 1, run = "t-1")
        assertEquals("working", q.delivery.label)
        assertEquals("unknown", Delivery("thinking").label)
        assertTrue(q.canCancel && q.isActive && q.runMayLive)
        // A version with its result is not cancelled.
        assertFalse(q.copy(results = listOf(AssistResult(2, 1, "done"))).canCancel)
        assertFalse(q.copy(delivery = Delivery.CANCEL_REQUESTED).canCancel)
        assertTrue(q.copy(delivery = Delivery.CANCEL_REQUESTED).isActive)
        assertFalse(q.copy(delivery = Delivery.RESULTED).isActive)
        val results = listOf(AssistResult(2, 2, "new"), AssistResult(1, 1, "old", stale = true))
        assertEquals(listOf(2L), q.copy(results = results).currentResults.map { it.version })
        assertEquals(listOf(1L), q.copy(results = results).earlierResults.map { it.version })
        val item = WorkItem("w-1", 1, "T", WorkState.OPEN, 1, 1, requests = listOf("q-1"))
        assertEquals("assistant working", assistNote(item, listOf(q)))
    }

    @Test
    fun askingNeedsAWorkspaceAndAShortPurpose() {
        assertEquals("Pick a workspace for the assistant.", CaptureDraft("Idea").askProblem)
        assertNull(CaptureDraft("Idea", "work").askProblem)
        assertEquals("Write something to save.", CaptureDraft(" ", "work").askProblem)
        assertTrue(assistProblem("work", "x".repeat(PURPOSE_MAX_BYTES + 1)) != null)
    }
}
