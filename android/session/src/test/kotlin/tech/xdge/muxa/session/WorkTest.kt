package tech.xdge.muxa.session

import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test
import tech.xdge.muxa.protocol.ErrorBody
import tech.xdge.muxa.protocol.ErrorCode
import tech.xdge.muxa.protocol.InboxEntry
import tech.xdge.muxa.protocol.Message
import tech.xdge.muxa.protocol.Proposal
import tech.xdge.muxa.protocol.Request
import tech.xdge.muxa.protocol.RoutineInfo
import tech.xdge.muxa.protocol.RoutineRun
import tech.xdge.muxa.protocol.RoutineTrigger
import java.time.Instant
import java.time.ZoneId

/** A bridge with Routines and stored entries, as the host answers them. */
private class WorkBridge : FakeBridge() {
    var routines = listOf(
        RoutineInfo("work", "digest", RoutineTrigger("08:00", "Europe/Berlin"), enabled = true, nextDue = 2000),
        RoutineInfo("work", "off", enabled = false),
    )
    var entries = listOf(entry("e-00000001", "proposal"), entry("e-00000002", "briefing"))

    /** False: no daemon, so `run-routine` is `unsupported`. */
    var routineDaemon = true
    private val _work = mutableListOf<Request>()

    /** The Routine and Inbox requests, in order. */
    val work: List<Request> get() = synchronized(_work) { _work.toList() }

    override fun answer(req: Request) {
        when (req) {
            is Request.ListRoutines -> {
                record(req)
                emit(Message.RoutineList(req.id, routines))
                emit(Message.Result(req.id))
            }
            is Request.ListInbox -> {
                record(req)
                emit(Message.InboxList(req.id, entries))
                emit(Message.Result(req.id))
            }
            is Request.RunRoutine -> runRoutine(req)
            else -> super.answer(req)
        }
    }

    private fun runRoutine(req: Request.RunRoutine) {
        record(req)
        if (!routineDaemon) {
            return refuse(req.id, ErrorCode.UNSUPPORTED, "Routines run in the muxa daemon. On the host, run: muxa-bridge daemon install")
        }
        val routine = routines.firstOrNull { it.workspace == req.workspace && it.name == req.routine }
        if (routine?.enabled != true) return refuse(req.id, ErrorCode.BAD_REQUEST, "the routine is disabled")
        if (routine.running != null) return refuse(req.id, ErrorCode.CONFLICT, "the routine is running")
        val started = routine.copy(running = RoutineRun("now-1", 1, "t-run1"))
        routines = routines.map { if (it == routine) started else it }
        emit(Message.RoutineSaved(req.id, started))
        emit(Message.Result(req.id))
    }

    private fun record(req: Request) = synchronized(_work) { _work += req }

    private fun refuse(id: Long, code: ErrorCode, message: String) = emit(Message.Result(id, ErrorBody(code, message)))
}

private fun entry(id: String, kind: String, disposition: String = "open", until: Long? = null) = InboxEntry(
    id, "work", "digest", id, kind, "Title $id", "Body", createdAt = 1, run = "t-run0", occurrence = "2026-11-03T08:00",
    disposition = disposition, until = until,
    proposal = if (kind == "proposal") Proposal("Close", "linear:ENG-1", "Done.", "sha256:00") else null,
)

class WorkTest {
    private suspend fun connected(bridge: WorkBridge): PaneSession {
        val session = PaneSession(oneThread())
        session.connect {
            bridge.start()
            bridge
        }
        return session
    }

    private suspend fun assertRefused(message: String? = null, block: suspend () -> Unit): PaneSession.Failure {
        try {
            block()
        } catch (e: PaneSession.Failure) {
            if (message != null) assertEquals(message, PaneSession.describe(e))
            return e
        }
        fail("expected a refusal")
        throw IllegalStateException()
    }

    @Test
    fun connectListsRoutinesAndEntriesAtWorkLevelOne() = runBlocking {
        val bridge = WorkBridge().apply { capabilities = listOf("task-records", "work:1") }
        val s = connected(bridge).state.value
        assertEquals(1, s.workLevel)
        assertTrue(s.canUseRoutines)
        assertEquals(listOf("digest", "off"), s.routines.map { it.name })
        // Below `work` level 3 the stored entries make no Inbox items.
        assertTrue(s.inboxItems.isEmpty())
    }

    @Test
    fun anOlderHostIsNeverAskedForRoutines() = runBlocking {
        val bridge = WorkBridge().apply { capabilities = listOf("task-records", "work:x") }
        val session = connected(bridge)
        assertEquals(0, session.state.value.workLevel)
        assertTrue(session.state.value.routines.isEmpty())
        assertTrue(bridge.work.isEmpty())
        assertRefused(PaneSession.ROUTINES_UNSUPPORTED) { session.runRoutine(bridge.routines[0]) }
        assertTrue(bridge.work.isEmpty())
    }

    @Test
    fun runNowKeepsTheRunAndShowsTheHostsRefusal() = runBlocking {
        val bridge = WorkBridge().apply { capabilities = listOf("work:2") }
        val session = connected(bridge)
        val saved = session.runRoutine(session.state.value.routines[0])
        assertEquals("t-run1", saved.running?.task)
        assertEquals("t-run1", session.state.value.routines[0].running?.task)
        // A second run while one is open is a conflict.
        assertRefused { session.runRoutine(session.state.value.routines[0]) }

        bridge.routineDaemon = false
        val refusal = assertRefused { session.runRoutine(session.state.value.routines[0]) }
        assertTrue(PaneSession.describe(refusal).contains("muxa-bridge daemon install"))
    }

    @Test
    fun laterTimesAreInTheFuture() {
        val tokyo = ZoneId.of("Asia/Tokyo")
        // 2026-11-03 23:30 in Tokyo.
        val now = Instant.ofEpochSecond(1_762_180_200)
        assertEquals(1_762_183_800L, LaterChoice.IN_AN_HOUR.until(now, tokyo))
        // 2026-11-04 09:00 in Tokyo.
        assertEquals(1_762_214_400L, LaterChoice.TOMORROW_MORNING.until(now, tokyo))
        assertNotNull(laterProblem(now.epochSecond, now.epochSecond))
        assertNotNull(laterProblem(now.epochSecond + 400 * 86_400, now.epochSecond))
        assertNull(laterProblem(now.epochSecond + 60, now.epochSecond))
    }
}
