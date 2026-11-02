package tech.xdge.muxa.session

import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test
import tech.xdge.muxa.protocol.ErrorCode
import tech.xdge.muxa.protocol.LoopCheck
import tech.xdge.muxa.protocol.LoopReason
import tech.xdge.muxa.protocol.LoopSpec
import tech.xdge.muxa.protocol.LoopState
import tech.xdge.muxa.protocol.Request
import tech.xdge.muxa.protocol.TaskLoop
import tech.xdge.muxa.protocol.TaskRecord
import tech.xdge.muxa.protocol.TaskStatus
import tech.xdge.muxa.protocol.WorkspaceChecks

/** The Task Loop through [PaneSession], and the words the app shows for it (ADR 0019). */
class TaskLoopTest {
    private val loops = listOf("task-records", "task-loop")

    private fun loop(state: LoopState, reason: LoopReason? = null, attempts: Int = 1, lastCheck: LoopCheck? = null) =
        TaskLoop("make check", 3, 600, state, attempts, 10, 10, reason, lastCheck)

    private fun record(id: String = "t-1", rev: Long = 1, loop: TaskLoop? = null) =
        TaskRecord(id, rev, "work", "Title", "Goal", TaskStatus.OPEN, 1, 1, loop = loop)

    private suspend fun connected(bridge: FakeBridge): PaneSession {
        val session = PaneSession(oneThread())
        session.connect {
            bridge.start()
            bridge
        }
        return session
    }

    @Test
    fun aStartedLoopReplacesTheListedRecord() = runBlocking {
        val bridge = FakeBridge().apply {
            capabilities = loops
            tasks = listOf(record(loop = loop(LoopState.STOPPED, LoopReason.LIMIT, attempts = 3)))
        }
        val session = connected(bridge)
        assertTrue(session.state.value.canLoop)
        val listed = session.state.value.tasks!!.records.single()
        val saved = session.startLoop(listed, LoopSpec("make check", 5))
        assertEquals(LoopState.ARMED, saved.loop?.state)
        assertEquals(3, saved.loop?.attempts)
        assertEquals(5, saved.loop?.maxAttempts)
        assertEquals(saved, session.state.value.tasks?.record("t-1"))
        val sent = bridge.received.last() as Request.StartLoop
        assertEquals("t-1", sent.task)
        assertEquals(LoopSpec("make check", 5), sent.loop)
        assertFalse(sent.reset)
    }

    @Test
    fun aResetCountsAttemptsFromZero() = runBlocking {
        val bridge = FakeBridge().apply {
            capabilities = loops
            tasks = listOf(record(loop = loop(LoopState.STOPPED, LoopReason.LIMIT, attempts = 3)))
        }
        val session = connected(bridge)
        val saved = session.startLoop(session.state.value.tasks!!.records.single(), LoopSpec("make check"), reset = true)
        assertEquals(0, saved.loop?.attempts)
        assertEquals(true, (bridge.received.last() as Request.StartLoop).reset)
    }

    @Test
    fun aStoppedLoopIsStoppedByTheUser() = runBlocking {
        val bridge = FakeBridge().apply {
            capabilities = loops
            tasks = listOf(record(loop = loop(LoopState.ARMED)))
        }
        val session = connected(bridge)
        val saved = session.stopLoop(session.state.value.tasks!!.records.single())
        assertEquals(LoopState.STOPPED, saved.loop?.state)
        assertEquals(LoopReason.USER, saved.loop?.reason)
        assertEquals(2L, session.state.value.tasks?.record("t-1")?.rev)
    }

    @Test
    fun aBridgeWithoutTaskLoopIsNeverAsked() = runBlocking {
        val bridge = FakeBridge().apply {
            capabilities = listOf("task-records")
            tasks = listOf(record())
        }
        val session = connected(bridge)
        for (call in listOf<suspend () -> Unit>(
            { session.startLoop(record(), LoopSpec("make check")) },
            { session.stopLoop(record()) },
            { session.createTask(TaskRecordDraft.Create("work", "T", "G", null, null, null, null, LoopSpec("make check"))) },
        )) {
            try {
                call()
                fail("loops need task-loop")
            } catch (e: PaneSession.Failure.Bridge) {
                assertEquals(ErrorCode.UNSUPPORTED, e.body.code)
                assertEquals(LOOP_NEEDS_DAEMON, PaneSession.describe(e))
            }
        }
        assertTrue(bridge.received.none { it is Request.StartLoop || it is Request.StopLoop || it is Request.CreateTask })
    }

    @Test
    fun theHostsRefusalSaysWhyAndKeepsTheRecord() = runBlocking {
        val bridge = FakeBridge().apply {
            capabilities = loops
            tasks = listOf(record())
        }
        val session = connected(bridge)
        try {
            session.startLoop(record(), LoopSpec("rm -rf /"))
            fail("the workspace does not allow it")
        } catch (e: PaneSession.Failure.Bridge) {
            assertEquals(ErrorCode.BAD_REQUEST, e.body.code)
        }
        bridge.daemon = false
        try {
            session.startLoop(record(), LoopSpec("make check"))
            fail("the daemon went away")
        } catch (e: PaneSession.Failure.Bridge) {
            assertEquals(LOOP_NEEDS_DAEMON, PaneSession.describe(e))
        }
        assertEquals(1L, session.state.value.tasks?.record("t-1")?.rev)
    }

    @Test
    fun aNewTaskCarriesItsLoop() = runBlocking {
        val bridge = FakeBridge().apply { capabilities = loops }
        val session = connected(bridge)
        val draft = TaskRecordDraft(
            workspace = "work", title = "T", goal = "G",
            loop = LoopDraft(WorkspaceChecks.Only(listOf("make check")), maxAttempts = 4),
        )
        val saved = session.createTask(draft.request!!)
        assertEquals(LoopSpec("make check", 4), (bridge.received.last { it is Request.CreateTask } as Request.CreateTask).loop)
        assertEquals(LoopState.ARMED, saved.loop?.state)
    }

    @Test
    fun aDraftFollowsTheWorkspacesChecks() {
        val listed = LoopDraft(WorkspaceChecks.Only(listOf("make check", "cargo test")))
        assertEquals("make check", listed.check)
        assertNull(listed.problem)
        assertEquals("Pick a check.", listed.copy(check = "make").problem)
        assertEquals(" make check", listed.copy(check = " make check").check)
        assertEquals("Max attempts must be 1 to 10.", listed.copy(maxAttempts = 11).problem)

        val any = LoopDraft(WorkspaceChecks.Any)
        assertEquals("Enter a check command.", any.problem)
        assertEquals(LoopSpec("npm test", 3), any.copy(check = "  npm test ").spec)
        assertEquals("The check command cannot contain control characters.", any.copy(check = "a\u0007b").problem)
        assertEquals("The check command is too long.", any.copy(check = "x".repeat(4097)).problem)

        assertEquals(LoopDraft.NO_CHECKS, LoopDraft(null).problem)
        assertEquals(LoopDraft.NO_CHECKS, TaskRecordDraft("work", "T", "G", loop = LoopDraft(null)).problem)
    }

    @Test
    fun theBadgeNamesTheStateAndAttempts() {
        assertEquals("Loop 1/3", loopBadge(loop(LoopState.ARMED)))
        assertEquals("Loop checking", loopBadge(loop(LoopState.CHECKING)))
        assertEquals("Loop paused", loopBadge(loop(LoopState.PAUSED, LoopReason.WAITING)))
        assertEquals("Loop passed", loopBadge(loop(LoopState.PASSED)))
        assertEquals("Loop stopped: limit", loopBadge(loop(LoopState.STOPPED, LoopReason.LIMIT)))
        assertEquals("Loop stopped: no agent", loopBadge(loop(LoopState.STOPPED, LoopReason.NO_AGENT)))
        assertEquals("Loop stopped", loopBadge(loop(LoopState.STOPPED, LoopReason.USER)))
        assertEquals("Loop: sleeping", loopBadge(loop(LoopState("sleeping"))))
    }

    @Test
    fun aPausedLoopSaysItResumesAfterTheTurn() {
        assertEquals(
            "The agent waits for you. The loop resumes after the agent finishes its turn.",
            loopExplanation(loop(LoopState.PAUSED, LoopReason.WAITING)),
        )
        assertEquals("Stopped: the check failed 3 times.", loopExplanation(loop(LoopState.STOPPED, LoopReason.LIMIT)))
        assertEquals("Stopped: a check ran longer than 10 min.", loopExplanation(loop(LoopState.STOPPED, LoopReason.TIMEOUT)))
        assertEquals("Stopped: nap.", loopExplanation(loop(LoopState.STOPPED, LoopReason("nap"))))
        assertEquals(
            "The check passed on attempt 2.",
            loopExplanation(loop(LoopState.PASSED, lastCheck = LoopCheck(2, 0, 30, 100))),
        )
        assertEquals("The host reports the state \"sleeping\".", loopExplanation(loop(LoopState("sleeping"))))
    }

    @Test
    fun theLastCheckSaysExitOrTimedOut() {
        assertEquals("Attempt 2 · exit 1 · 42 s · 5 min ago", lastCheckLine(LoopCheck(2, 1, 42, 1_000), now = 1_300))
        assertEquals("Attempt 3 · timed out · 10 min · <1 min ago", lastCheckLine(LoopCheck(3, null, 600, 1_000), now = 1_010))
    }

    @Test
    fun onlyAnArmedCheckingOrPausedLoopCanBeStopped() {
        assertTrue(loop(LoopState.ARMED).isActive)
        assertTrue(loop(LoopState.PAUSED).isActive)
        assertFalse(loop(LoopState.PASSED).isActive)
        assertFalse(loop(LoopState.STOPPED).isActive)
        assertFalse(loop(LoopState("sleeping")).isKnownState)
    }
}
