package tech.xdge.muxa.session

import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test
import tech.xdge.muxa.protocol.Agent
import tech.xdge.muxa.protocol.AgentState
import tech.xdge.muxa.protocol.ErrorBody
import tech.xdge.muxa.protocol.ErrorCode
import tech.xdge.muxa.protocol.InboxAction
import tech.xdge.muxa.protocol.InboxChange
import tech.xdge.muxa.protocol.InboxEntry
import tech.xdge.muxa.protocol.Message
import tech.xdge.muxa.protocol.Pane
import tech.xdge.muxa.protocol.Proposal
import tech.xdge.muxa.protocol.Request
import tech.xdge.muxa.protocol.Role
import tech.xdge.muxa.protocol.RoutineInfo
import tech.xdge.muxa.protocol.RoutineRun
import tech.xdge.muxa.protocol.RoutineTrigger
import tech.xdge.muxa.protocol.Session
import tech.xdge.muxa.protocol.TaskInbox
import tech.xdge.muxa.protocol.TaskRecord
import tech.xdge.muxa.protocol.TaskStatus
import java.time.Instant
import java.time.ZoneId

/** A bridge with Routines, stored entries and the Task's Inbox place, as the host answers them. */
private class WorkBridge : FakeBridge() {
    var routines = listOf(
        RoutineInfo("work", "digest", RoutineTrigger("08:00", "Europe/Berlin"), enabled = true, nextDue = 2000),
        RoutineInfo("work", "off", enabled = false),
    )
    var entries = listOf(entry("e-00000001", "proposal"), entry("e-00000002", "briefing"))

    /** False: no daemon, so `run-routine` is `unsupported`. */
    var routineDaemon = true
    private val _work = mutableListOf<Request>()

    /** The Routine, entry and place requests, in order. */
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
            is Request.UpdateInbox -> updateEntry(req)
            is Request.UpdateTask -> if (req.inbox != null) place(req, req.inbox!!) else super.answer(req)
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

    private fun updateEntry(req: Request.UpdateInbox) {
        record(req)
        val current = entries.firstOrNull { it.id == req.entry }
        if (current == null || (current.disposition !in listOf("open", "later") && req.action != InboxAction.APPROVE)) {
            return refuse(req.id, ErrorCode.BAD_REQUEST, "the entry is closed")
        }
        val saved = when (req.action) {
            InboxAction.APPROVE -> {
                if (current.kind != "proposal") return refuse(req.id, ErrorCode.BAD_REQUEST, "a briefing has nothing to approve")
                if (current.task != null) {
                    current
                } else {
                    val task = TaskRecord("t-ok${tasks.size}", 1, "work", current.title, "g", TaskStatus.OPEN, 1, 1)
                    tasks = tasks + task
                    current.copy(task = task.id, disposition = "approved")
                }
            }
            InboxAction.DISMISS -> current.copy(disposition = "dismissed")
            InboxAction.LATER -> current.copy(disposition = "later", until = req.until)
        }
        entries = entries.map { if (it.id == saved.id) saved else it }
        emit(Message.InboxSaved(req.id, saved))
        emit(Message.Result(req.id))
    }

    private fun place(req: Request.UpdateTask, change: InboxChange) {
        record(req)
        val current = tasks.firstOrNull { it.id == req.task }
        if (current?.rev != req.rev) return refuse(req.id, ErrorCode.CONFLICT, "stale")
        val saved = current.copy(rev = current.rev + 1, inbox = TaskInbox(if (change.down) 5000 else null, change.until, 5))
        tasks = tasks.map { if (it.id == saved.id) saved else it }
        emit(Message.TaskSaved(req.id, saved))
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

    private fun record(id: String, status: TaskStatus = TaskStatus.OPEN, inbox: TaskInbox? = null) =
        TaskRecord(id, 1, "work", "Title $id", "Goal", status, 1, 9, inbox = inbox)

    private fun session(taskId: String, state: AgentState, shellForeground: Boolean = false) = Session(
        id = "$$taskId", name = "s-$taskId", taskId = taskId,
        panes = listOf(Pane("%$taskId", "@1", 0, Role.AGENT, 80, 24, "claude", "/w", false, shellForeground, Agent(state, 100))),
    )

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
        assertEquals(listOf("e-00000001", "e-00000002"), s.inboxEntries.map { it.id })
    }

    @Test
    fun anOlderHostIsNeverAskedForRoutines() = runBlocking {
        val bridge = WorkBridge().apply { capabilities = listOf("task-records", "work:x") }
        val session = connected(bridge)
        assertEquals(0, session.state.value.workLevel)
        assertTrue(session.state.value.routines.isEmpty())
        assertTrue(bridge.work.isEmpty())
        assertRefused(PaneSession.ROUTINES_UNSUPPORTED) { session.runRoutine(bridge.routines[0]) }
        assertRefused { session.dismiss(bridge.entries[1]) }
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
    fun approvingCreatesOneTaskAndListsIt() = runBlocking {
        val bridge = WorkBridge().apply { capabilities = listOf("task-records", "work:1") }
        val session = connected(bridge)
        val approved = session.approve(session.state.value.inboxEntries[0])
        assertEquals("approved", approved.disposition)
        val task = checkNotNull(approved.task)
        assertEquals(listOf(task), session.state.value.tasks?.records?.map { it.id })
        assertEquals("approved", session.state.value.inboxEntries[0].disposition)
        // The approved entry leaves the list.
        assertEquals(listOf("e-00000002"), inboxRows(emptyList(), session.state.value.inboxEntries, emptyList(), emptyList()).map { it.id })
        // A second approval of the same entry creates nothing.
        runCatching { session.approve(approved) }
        assertEquals(1, bridge.tasks.size)
    }

    @Test
    fun dismissAndLaterSendTheirAction() = runBlocking {
        val bridge = WorkBridge().apply { capabilities = listOf("work:1") }
        val session = connected(bridge)
        session.dismiss(session.state.value.inboxEntries[1])
        session.later(session.state.value.inboxEntries[0], 9000)
        assertEquals(listOf("later", "dismissed"), session.state.value.inboxEntries.map { it.disposition })
        assertTrue(bridge.work.any { it is Request.UpdateInbox && it.entry == "e-00000001" && it.action == InboxAction.LATER && it.until == 9000L })
        // A closed entry is refused by the host, and the list is read again.
        val lists = bridge.work.count { it is Request.ListInbox }
        assertRefused { session.dismiss(session.state.value.inboxEntries[1]) }
        assertEquals(lists + 1, bridge.work.count { it is Request.ListInbox })
    }

    @Test
    fun moveDownAndLaterNeedTaskInbox() = runBlocking {
        val bridge = WorkBridge().apply {
            capabilities = listOf("task-records")
            tasks = listOf(record("t-1"))
        }
        val session = connected(bridge)
        assertFalse(session.state.value.canPlaceTasks)
        assertRefused(PaneSession.PLACE_UNSUPPORTED) { session.place(session.state.value.tasks!!.records[0], InboxChange(down = true)) }
        assertTrue(bridge.work.isEmpty())
    }

    @Test
    fun moveDownSavesThePlaceAndAStaleRevisionConflicts() = runBlocking {
        val bridge = WorkBridge().apply {
            capabilities = listOf("task-records", "task-inbox")
            tasks = listOf(record("t-1"))
        }
        val session = connected(bridge)
        val stale = session.state.value.tasks!!.records[0]
        val saved = session.place(stale, InboxChange(down = true))
        assertEquals(5000L, saved.inbox?.order)
        assertEquals(2L, session.state.value.tasks!!.records[0].rev)
        val conflict = assertRefused { session.place(stale, InboxChange(until = 9000)) }
        assertEquals(ErrorCode.CONFLICT, (conflict as PaneSession.Failure.Bridge).body.code)
        assertTrue(PaneSession.describe(conflict).contains("changed on another device"))
    }

    @Test
    fun movedDownTasksComeAfterEntriesAndLaterHides() {
        val moved = record("t-down", TaskStatus.REVIEW, TaskInbox(order = 1, since = 200))
        val later = record("t-later", TaskStatus.REVIEW, TaskInbox(until = 500, since = 200))
        val plain = record("t-plain", TaskStatus.REVIEW)
        val sessions = listOf(
            session("t-down", AgentState.IDLE), session("t-later", AgentState.IDLE),
            session("t-plain", AgentState.IDLE), session("t-run", AgentState.WAITING),
        )
        val queue = triageQueue(listOf(moved, later, plain), sessions, now = 300)
        val routines = listOf(RoutineInfo("w", "r", enabled = true, running = RoutineRun("now-1", 1, "t-run")))
        val entries = listOf(
            entry("e-1", "briefing"), entry("e-2", "proposal", "dismissed"),
            entry("e-3", "proposal", "later", until = 400), entry("e-4", "briefing", "later", until = 200),
        )
        val rows = inboxRows(queue, entries, routines, sessions, now = 300)
        assertEquals(listOf("t-plain", "t-run", "e-1", "e-4", "t-down"), rows.map { it.id })
    }

    @Test
    fun aRoutineRunWithoutAWaitingAgentIsNoNeed() {
        val routines = listOf(RoutineInfo("w", "r", enabled = true, running = RoutineRun("now-1", 1, "t-run")))
        assertTrue(routines.needs(listOf(session("t-run", AgentState.RUNNING))).isEmpty())
        assertTrue(routines.needs(listOf(session("t-run", AgentState.WAITING, shellForeground = true))).isEmpty())
        assertEquals(listOf("t-run"), routines.needs(listOf(session("t-run", AgentState.WAITING))).map { it.task })
    }

    @Test
    fun unknownEntriesShowWithoutActions() {
        val unknown = entry("e-9", "digest")
        assertTrue(unknown.isShown(now = 1))
        assertFalse(unknown.hasKnownActions)
        assertFalse(entry("e-8", "proposal", "approved").hasKnownActions)
        assertNull(entry("e-7", "briefing").proposal)
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
