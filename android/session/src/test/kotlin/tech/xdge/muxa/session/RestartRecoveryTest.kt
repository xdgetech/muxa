package tech.xdge.muxa.session

import kotlinx.coroutines.runBlocking
import org.junit.Assert.*
import org.junit.Test
import tech.xdge.muxa.protocol.*

class RestartRecoveryTest {
    private val all = RestartRecoveryAction.entries.map { it.wireName } + "future-action"

    @Test fun actionsRequireKnownPhaseAndHostOffer() {
        assertEquals(listOf(RestartRecoveryAction.CANCEL_PREPARED), RestartOperation("op", "prepared", actions = all).recoveryActions)
        assertEquals(listOf(RestartRecoveryAction.LEAVE_UNCONFIRMED), RestartOperation("op", "dispatching", actions = all).recoveryActions)
        assertEquals(listOf(RestartRecoveryAction.FINISH_REPLACEMENT, RestartRecoveryAction.LEAVE_UNCONFIRMED), RestartOperation("op", "replaced", actions = all).recoveryActions)
        assertEquals(listOf(RestartRecoveryAction.LEAVE_UNCONFIRMED), RestartOperation("op", "delivering", actions = all).recoveryActions)
        for (phase in listOf("future", "finalized", "resolved")) assertTrue(RestartOperation("op", phase, actions = all).recoveryActions.isEmpty())
        assertTrue(RestartOperation("op", "prepared").recoveryActions.isEmpty())
        assertTrue(RestartOperation("", "prepared", actions = all).recoveryActions.isEmpty())
    }

    private suspend fun connected(capable: Boolean = true, phase: String = "prepared"): Triple<PaneSession, FakeBridge, TaskRecord> {
        val bridge = FakeBridge().apply {
            capabilities = listOf("task-records", "task-handoff") + if (capable) listOf("task-restart-recovery") else emptyList()
            tasks = listOf(TaskRecord("t-1", 7, "work", "Title", "Goal", TaskStatus.OPEN, 1, 1,
                session = "s", restartOperation = RestartOperation("op-1", phase, actions = all)))
        }
        val session = PaneSession(oneThread())
        session.connect { bridge.start(); bridge }
        return Triple(session, bridge, session.state.value.tasks!!.records.single())
    }

    @Test fun explicitRecoverySendsDisplayedIdentityAndNoRestart() = runBlocking {
        val (session, bridge, displayed) = connected()
        val saved = session.recoverRestart(displayed, "op-1", RestartRecoveryAction.CANCEL_PREPARED)
        assertNull(saved.restartOperation)
        val request = bridge.received.filterIsInstance<Request.RecoverRestart>().single()
        assertEquals(displayed.id, request.task)
        assertEquals(displayed.rev, request.rev)
        assertEquals("op-1", request.operation)
        assertEquals(RestartRecoveryAction.CANCEL_PREPARED, request.action)
        assertTrue(bridge.received.none { it is Request.RestartAgent })
    }

    @Test fun staleRevisionConflictsAndRefreshesWithoutRetry() = runBlocking {
        val (session, bridge, displayed) = connected()
        bridge.tasks = listOf(displayed.copy(rev = displayed.rev + 1))
        try {
            session.recoverRestart(displayed, "op-1", RestartRecoveryAction.CANCEL_PREPARED)
            fail("stale displayed revision must conflict")
        } catch (e: PaneSession.Failure.Bridge) { assertEquals(ErrorCode.CONFLICT, e.body.code) }
        assertEquals(displayed.rev + 1, session.state.value.tasks?.records?.single()?.rev)
        assertEquals(1, bridge.received.filterIsInstance<Request.RecoverRestart>().size)
    }

    @Test fun oldHostUnknownOrChangedPhaseCannotReceiveRecovery() = runBlocking {
        for ((capable, phase) in listOf(false to "prepared", true to "future")) {
            val (session, bridge, displayed) = connected(capable, phase)
            try {
                session.recoverRestart(displayed, "op-1", RestartRecoveryAction.CANCEL_PREPARED)
                fail("unavailable recovery action must fail")
            } catch (_: PaneSession.Failure.Bridge) { }
            assertTrue(bridge.received.none { it is Request.RecoverRestart })
        }
        val (session, bridge, displayed) = connected()
        bridge.tasks = listOf(displayed.copy(restartOperation = displayed.restartOperation!!.copy(phase = "future")))
        session.loadTasks()
        try {
            session.recoverRestart(displayed, "op-1", RestartRecoveryAction.CANCEL_PREPARED)
            fail("changed phase must invalidate open confirmation")
        } catch (e: PaneSession.Failure.Bridge) { assertEquals(ErrorCode.CONFLICT, e.body.code) }
        try {
            session.restartAgent(displayed, true)
            fail("unresolved operation must block ordinary restart")
        } catch (e: PaneSession.Failure.Bridge) { assertEquals(ErrorCode.CONFLICT, e.body.code) }
        assertTrue(bridge.received.none { it is Request.RecoverRestart || it is Request.RestartAgent })
    }

    @Test fun unconfirmedRunNeverClaimsToBeRunning() {
        val record = TaskRecord("t-1", 1, "work", "Title", "Goal", TaskStatus.OPEN, 1, 1,
            runs = listOf(AgentRun(1, startedAt = 1, reason = RunReason.RESTART, outcome = "unconfirmed")))
        assertTrue(runRows(record, true, 3).single().detail.endsWith("unconfirmed outcome"))
        assertTrue(runRows(record.copy(runs = listOf(record.runs.single().copy(outcome = "future"))), true, 3).single().detail.endsWith("unknown outcome"))
    }
}
