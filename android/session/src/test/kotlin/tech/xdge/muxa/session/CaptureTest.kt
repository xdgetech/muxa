package tech.xdge.muxa.session

import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import tech.xdge.muxa.protocol.Capture
import tech.xdge.muxa.protocol.CaptureSource
import tech.xdge.muxa.protocol.CaptureState
import tech.xdge.muxa.protocol.ErrorBody
import tech.xdge.muxa.protocol.ErrorCode
import tech.xdge.muxa.protocol.Message
import tech.xdge.muxa.protocol.Request
import tech.xdge.muxa.protocol.SortAction
import tech.xdge.muxa.protocol.WorkAction
import tech.xdge.muxa.protocol.WorkItem
import tech.xdge.muxa.protocol.WorkState

/** A host at `work` level 9 with one Work item; it keeps one Capture per `clientId`, as PROTOCOL.md says. */
private class CaptureBridge(level: Int = 9) : FakeBridge() {
    var work = listOf(WorkItem("w-00000001", 3, "Plan the offsite", WorkState.OPEN, 1, 1))
    var captures = listOf<Capture>()

    init {
        capabilities = listOf("task-records", "work:$level")
    }

    val captureRequests get() = received.filterIsInstance<Request.Capture>()

    override fun answer(req: Request) {
        when (req) {
            is Request.ListInbox -> {
                emit(Message.InboxList(req.id, emptyList(), work = work, captures = captures.filter { it.state == CaptureState.RECEIVED }))
                emit(Message.Result(req.id))
            }
            is Request.Capture -> {
                val stored = captures.firstOrNull { it.clientId == req.clientId }
                    ?: Capture("c-%08x".format(captures.size + 1), 1, req.clientId, req.text, req.source ?: CaptureSource.APP, CaptureState.RECEIVED, 5, 5, req.workspace)
                        .also { captures = captures + it }
                emit(Message.CaptureSaved(req.id, stored))
                emit(Message.Result(req.id))
            }
            is Request.SortCapture -> {
                val c = captures.first { it.id == req.capture }
                if (c.rev != req.rev) return refuse(req.id, ErrorCode.CONFLICT, "${c.id} changed")
                val sorted = c.copy(rev = c.rev + 1, state = CaptureState.SORTED, work = listOfNotNull(req.work))
                captures = captures.map { if (it.id == c.id) sorted else it }
                emit(Message.CaptureSaved(req.id, sorted))
                emit(Message.Result(req.id))
            }
            is Request.UpdateWork -> {
                val w = work.first { it.id == req.work }
                if (w.rev != req.rev) return refuse(req.id, ErrorCode.CONFLICT, "${w.id} changed")
                val saved = when (req.action) {
                    WorkAction.LATER -> w.copy(rev = w.rev + 1, laterAt = 9)
                    WorkAction.COMPLETE -> w.copy(rev = w.rev + 1, state = WorkState.DONE)
                    else -> return refuse(req.id, ErrorCode.BAD_REQUEST, "not here")
                }
                work = work.map { if (it.id == w.id) saved else it }
                emit(Message.WorkSaved(req.id, saved))
                emit(Message.Result(req.id))
            }
            else -> super.answer(req)
        }
    }

    private fun refuse(id: Long, code: ErrorCode, message: String) = emit(Message.Result(id, ErrorBody(code, message)))
}

class CaptureTest {
    @get:Rule
    val folder = TemporaryFolder()

    private suspend fun connected(bridge: CaptureBridge): PaneSession {
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
    fun connectListsWorkAndCapturesAtLevelNine() = runBlocking {
        val bridge = CaptureBridge().apply {
            captures = listOf(Capture("c-00000009", 1, "x", "Idea", CaptureSource.TMUX, CaptureState.RECEIVED, 1, 1))
        }
        val s = connected(bridge).state.value
        assertTrue(s.canCapture)
        assertEquals(listOf("w-00000001"), s.work.map { it.id })
        assertEquals(listOf("c-00000009"), s.captures.map { it.id })
    }

    @Test
    fun anOlderHostListsNoWorkAndIsNeverSentACapture() = runBlocking {
        val bridge = CaptureBridge(level = 8)
        val session = connected(bridge)
        assertTrue(session.state.value.work.isEmpty())
        val e = refusal { session.capture("id-1", "Idea", null) }
        assertEquals(PaneSession.CAPTURE_UNSUPPORTED, PaneSession.describe(e))
        assertTrue(bridge.captureRequests.isEmpty())
    }

    @Test
    fun aRepeatedClientIdReturnsTheStoredCapture() = runBlocking {
        val bridge = CaptureBridge()
        val session = connected(bridge)
        val first = session.capture("id-1", "Look into the upload test", "work")
        val again = session.capture("id-1", "Look into the upload test", "work")
        assertEquals(first.id, again.id)
        assertEquals(1, bridge.captures.size)
        assertEquals(CaptureSource.APP, bridge.captureRequests.first().source)
        // The Inbox is listed again, so Unsorted shows it.
        assertEquals(listOf(first.id), session.state.value.captures.map { it.id })
    }

    @Test
    fun aStaleRevIsWorkChangedAndTheListIsReadAgain() = runBlocking {
        val bridge = CaptureBridge()
        val session = connected(bridge)
        val seen = session.state.value.work.single()
        // Another device moved it down first.
        bridge.work = bridge.work.map { it.copy(rev = it.rev + 1, laterAt = 4) }
        val e = refusal { session.updateWork(seen, WorkAction.COMPLETE) }
        assertTrue(e is PaneSession.Failure.WorkChanged)
        assertEquals(PaneSession.WORK_CHANGED, PaneSession.describe(e))
        assertEquals(4L, session.state.value.work.single().laterAt)
        // With the new rev it goes through.
        assertEquals(WorkState.DONE, session.updateWork(session.state.value.work.single(), WorkAction.COMPLETE).state)
    }

    @Test
    fun sortingSendsTheRevAndLeavesUnsorted() = runBlocking {
        val bridge = CaptureBridge()
        val session = connected(bridge)
        session.capture("id-1", "Idea", null)
        val c = session.state.value.captures.single()
        val sorted = session.sortCapture(c, SortAction.LINK, session.state.value.work.single())
        assertEquals(listOf("w-00000001"), sorted.work)
        val sent = bridge.received.filterIsInstance<Request.SortCapture>().single()
        assertEquals(c.rev, sent.rev)
        assertEquals("w-00000001", sent.work)
        assertTrue(session.state.value.captures.isEmpty())
    }

    @Test
    fun aTaskForAWorkItemSendsItsRevOnlyAtLevelNine() = runBlocking {
        val draft = TaskRecordDraft.forWork(WorkItem("w-00000001", 3, "Plan the offsite", WorkState.OPEN, 1, 1), listOf())
            .copy(workspace = "work")
        val bridge = CaptureBridge()
        connected(bridge).createTask(draft.request!!)
        val sent = bridge.received.filterIsInstance<Request.CreateTask>().single()
        assertEquals("w-00000001", sent.work)
        assertEquals(3L, sent.workRev)

        val older = CaptureBridge(level = 8)
        connected(older).createTask(draft.request!!)
        val plain = older.received.filterIsInstance<Request.CreateTask>().single()
        assertNull(plain.work)
        assertNull(plain.workRev)
    }

    @Test
    fun aCaptureSentWhileDisconnectedIsNotConnected() = runBlocking {
        val session = connected(CaptureBridge())
        session.disconnect()
        val e = refusal { session.capture("id-1", "Idea", null) }
        assertTrue(e is PaneSession.Failure.NotConnected)
        // Not a refusal: it waits for the next connection.
        assertNull(unsentReason(e))
    }

    @Test
    fun unsentCapturesSurviveARestartAndKeepTheirClientId() {
        val file = folder.root.resolve("unsent.json")
        val store = UnsentCaptures(file)
        store.add(UnsentCapture("id-1", "host-a", "First idea", createdAt = 1))
        store.add(UnsentCapture("id-2", "host-b", "Second idea", "work", createdAt = 2))
        store.refused("id-2", "too long")

        val reopened = UnsentCaptures(file)
        assertEquals(listOf("id-1"), reopened.forHost("host-a").map { it.clientId })
        assertEquals("too long", reopened.forHost("host-b").single().error)
        // The same clientId replaces, never doubles.
        reopened.add(UnsentCapture("id-1", "host-a", "First idea", createdAt = 1))
        assertEquals(2, reopened.contents.value.size)
        reopened.remove("id-1")
        assertEquals(listOf("id-2"), UnsentCaptures(file).contents.value.map { it.clientId })
    }
}
