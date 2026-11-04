package tech.xdge.muxa.session

import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test
import tech.xdge.muxa.protocol.Agent
import tech.xdge.muxa.protocol.AgentRun
import tech.xdge.muxa.protocol.AgentState
import tech.xdge.muxa.protocol.ErrorCode
import tech.xdge.muxa.protocol.Pane
import tech.xdge.muxa.protocol.Request
import tech.xdge.muxa.protocol.Role
import tech.xdge.muxa.protocol.RunReason
import tech.xdge.muxa.protocol.Session
import tech.xdge.muxa.protocol.TaskRecord
import tech.xdge.muxa.protocol.TaskStatus

class HandoffTest {
    private fun record(id: String, session: String? = "s", runs: List<AgentRun> = emptyList()) =
        TaskRecord(id, 1, "work", "Title", "Goal", TaskStatus.OPEN, 1, 1, session = session, runs = runs)

    private suspend fun connected(bridge: FakeBridge): PaneSession {
        val session = PaneSession(oneThread())
        session.connect {
            bridge.start()
            bridge
        }
        return session
    }

    @Test
    fun aRestartReplacesTheListedRecord() = runBlocking {
        val bridge = FakeBridge().apply {
            capabilities = listOf("task-records", "task-handoff")
            tasks = listOf(record("t-1", runs = listOf(AgentRun(1, "claude-code", 10, reason = RunReason.START))))
        }
        val session = connected(bridge)
        val saved = session.restartAgent(session.state.value.tasks!!.records.single(), force = false)
        assertEquals(listOf(1L, 2L), saved.runs.map { it.id })
        assertEquals(50L, saved.runs.first().endedAt)
        assertEquals(saved, session.state.value.tasks?.record("t-1"))
        assertEquals(false, (bridge.received.last() as? Request.RestartAgent)?.force)
    }

    @Test
    fun aRunningAgentIsRefusedWithoutForceAndRestartedWithIt() = runBlocking {
        val bridge = FakeBridge().apply {
            capabilities = listOf("task-records", "task-handoff")
            tasks = listOf(record("t-1", session = "running"))
        }
        val session = connected(bridge)
        val listed = session.state.value.tasks!!.records.single()
        try {
            session.restartAgent(listed, force = false)
            fail("a running agent needs force")
        } catch (e: PaneSession.Failure.Bridge) {
            assertEquals(ErrorCode.BAD_REQUEST, e.body.code)
        }
        assertEquals(1L, session.state.value.tasks?.record("t-1")?.rev)
        assertEquals(1, session.restartAgent(listed, force = true).runs.size)
    }

    @Test
    fun aTaskFailedSaysWhy() = runBlocking {
        val bridge = FakeBridge().apply {
            capabilities = listOf("task-records", "task-handoff")
            tasks = listOf(record("t-1", session = null))
        }
        val session = connected(bridge)
        try {
            session.restartAgent(session.state.value.tasks!!.records.single(), force = false)
            fail("no session runs the agent")
        } catch (e: PaneSession.Failure.Bridge) {
            assertEquals(ErrorCode.TASK_FAILED, e.body.code)
            assertEquals("No session runs this task's agent.", PaneSession.describe(e))
        }
    }

    @Test
    fun aBridgeWithoutHandoffIsNeverAsked() = runBlocking {
        val bridge = FakeBridge().apply {
            capabilities = listOf("task-records")
            tasks = listOf(record("t-1"))
        }
        val session = connected(bridge)
        try {
            session.restartAgent(session.state.value.tasks!!.records.single(), force = true)
            fail("the bridge cannot restart agents")
        } catch (e: PaneSession.Failure.Bridge) {
            assertEquals(ErrorCode.UNSUPPORTED, e.body.code)
        }
        assertTrue(bridge.received.none { it is Request.RestartAgent })
    }

    private val runs = listOf(
        AgentRun(1, "claude-code", startedAt = 1_000, endedAt = 1_000 + 3_900, agentSession = "a", reason = RunReason.START),
        AgentRun(2, null, startedAt = 4_900, reason = RunReason.RESTART),
    )

    @Test
    fun runsAreNewestFirstWithWhatIsKnown() {
        val rows = runRows(record("t-1", runs = runs), agentReported = true, now = 4_900 + 2_520)
        assertEquals(listOf("Run 2 · unknown agent", "Run 1 · claude-code"), rows.map { it.title })
        assertEquals("Restarted · 42 min ago · still running", rows[0].detail)
        assertEquals("Started · 1 h 47 min ago · 1 h 5 min", rows[1].detail)
    }

    @Test
    fun anOpenRunIsNotRunningWithoutALiveAgent() {
        val rows = runRows(record("t-1", runs = runs), agentReported = false, now = 4_930)
        assertEquals("Restarted · <1 min ago · end not recorded", rows[0].detail)
        // An older run without an end is never the running one.
        val old = record("t-1", runs = listOf(runs[1].copy(id = 1), runs[1].copy(id = 2, reason = RunReason("resume"))))
        val both = runRows(old, agentReported = true, now = 4_930)
        assertEquals("end not recorded", both[1].detail.substringAfterLast(" · "))
        assertTrue(both[0].detail.startsWith("resume · "))
    }

    @Test
    fun durationsReadLikeIos() {
        assertEquals("<1 min", duration(59))
        assertEquals("42 min", duration(42 * 60 + 5))
        assertEquals("2 h", duration(7_200))
        assertEquals("1 h 5 min", duration(3_900))
    }

    @Test
    fun onlyARestartWritesAHandoff() {
        assertFalse(record("t-1", runs = runs.take(1)).hasHandoff)
        assertTrue(record("t-1", runs = runs).hasHandoff)
    }

    @Test
    fun anUnobservedReplacementNeedsForceWithoutBeingShownAsRunning() {
        val replacement = record("t-1", runs = runs.reversed())
        val unknown = taskItem(replacement, emptyList())
        assertTrue(unknown.restartNeedsForce)
        assertEquals(null, unknown.agent)
        assertEquals(InboxGroup.NO_AGENT, unknown.group)
        val pane = Pane("%1", "@1", 0, Role.AGENT, 80, 24, "claude", "/w", false, false, Agent(AgentState.IDLE, 1))
        assertFalse(taskItem(replacement, listOf(Session("$1", "s", taskId = "t-1", panes = listOf(pane)))).restartNeedsForce)
        val newerStart = replacement.copy(runs = runs + AgentRun(3, null, 5_000, reason = RunReason.START))
        assertFalse(taskItem(newerStart, emptyList()).restartNeedsForce)
    }

    @Test
    fun anUnobservedReplacementCanBeExplicitlyStoppedAndRestarted() = runBlocking {
        val bridge = FakeBridge().apply {
            capabilities = listOf("task-records", "task-handoff")
            tasks = listOf(record("t-1", runs = runs))
        }
        val session = connected(bridge)
        val listed = session.state.value.tasks!!.records.single()
        val item = taskItem(listed, emptyList())
        assertTrue(item.restartNeedsForce)
        val saved = session.restartAgent(listed, force = item.restartNeedsForce)
        assertEquals(3, saved.runs.size)
        assertEquals(true, (bridge.received.last() as? Request.RestartAgent)?.force)
    }

    @Test
    fun onlyARunningAgentNeedsForce() {
        fun item(state: AgentState?, shellForeground: Boolean = false): TaskItem {
            val pane = Pane("%1", "@1", 0, Role.AGENT, 80, 24, "claude", "/w", false, shellForeground, state?.let { Agent(it, 1) })
            return taskItem(record("t-1"), listOf(Session("$1", "s", taskId = "t-1", panes = listOf(pane))))
        }
        assertTrue(item(AgentState.RUNNING).restartNeedsForce)
        assertFalse(item(AgentState.WAITING).restartNeedsForce)
        assertFalse(item(null).restartNeedsForce)
        // A shell in front: the adapter's state is not believed.
        assertFalse(item(AgentState.RUNNING, shellForeground = true).restartNeedsForce)
    }
}
