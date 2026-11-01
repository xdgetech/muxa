package tech.xdge.muxa.session

import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test
import tech.xdge.muxa.protocol.ErrorCode
import tech.xdge.muxa.protocol.Message
import tech.xdge.muxa.protocol.Request
import tech.xdge.muxa.protocol.TaskRecord
import tech.xdge.muxa.protocol.TaskStatus

class TaskRecordsTest {
    private fun record(id: String, rev: Long = 1, status: TaskStatus = TaskStatus.OPEN) =
        TaskRecord(id, rev, "work", "Title $id", "Goal", status, 1, 1)

    private suspend fun connected(bridge: FakeBridge): PaneSession {
        val session = PaneSession(oneThread())
        session.connect {
            bridge.start()
            bridge
        }
        return session
    }

    @Test
    fun theTaskListArrivesWithTheConnect() = runBlocking {
        val bridge = FakeBridge().apply {
            capabilities = listOf("task-records")
            tasks = listOf(record("t-1"))
        }
        val session = connected(bridge)
        val board = session.state.value.tasks
        assertEquals(listOf("work"), board?.workspaces?.map { it.name })
        assertEquals(listOf("t-1"), board?.records?.map { it.id })
    }

    @Test
    fun aBridgeWithoutTaskRecordsIsNeverAsked() = runBlocking {
        val bridge = FakeBridge().apply { tasks = listOf(record("t-1")) }
        val session = connected(bridge)
        assertNull(session.state.value.tasks)
        assertTrue(bridge.received.none { it is Request.ListTasks })
    }

    @Test
    fun sessionsChangedListsTheTasksAgain() = runBlocking {
        val bridge = FakeBridge().apply { capabilities = listOf("task-records") }
        val session = connected(bridge)
        bridge.tasks = listOf(record("t-2"))
        bridge.emit(Message.SessionsChanged)
        waitUntil { session.state.value.tasks?.records?.map { it.id } == listOf("t-2") }
    }

    @Test
    fun aCreatedTaskJoinsTheListAndItsSessionIsListed() = runBlocking {
        val bridge = FakeBridge().apply { capabilities = listOf("task-records") }
        val session = connected(bridge)
        val draft = TaskRecordDraft(workspace = "work", title = " Plan ", goal = "Write it").request!!
        val saved = session.createTask(draft)
        assertEquals("Plan", (bridge.received.last { it is Request.CreateTask } as Request.CreateTask).title)
        assertEquals(listOf(saved.id), session.state.value.tasks?.records?.map { it.id })
        assertEquals(saved.id, session.state.value.sessions.last().taskId)
    }

    @Test
    fun aFailedLaunchKeepsTheRecordWithItsNote() = runBlocking {
        val bridge = FakeBridge().apply { capabilities = listOf("task-records") }
        val session = connected(bridge)
        val draft = TaskRecordDraft(workspace = "work", title = "T", goal = "G", repoDir = "/src/app", branch = "taken")
        try {
            session.createTask(draft.request!!)
            fail("the launch should fail")
        } catch (e: PaneSession.Failure.LaunchFailed) {
            assertEquals("the branch is taken", e.record.note)
        }
        assertEquals("the branch is taken", session.state.value.tasks?.records?.single()?.note)
    }

    @Test
    fun anUpdateTakesTheSavedRecord() = runBlocking {
        val bridge = FakeBridge().apply {
            capabilities = listOf("task-records")
            tasks = listOf(record("t-1"))
        }
        val session = connected(bridge)
        val saved = session.updateTask(session.state.value.tasks!!.records.single(), TaskStatus.DONE)
        assertEquals(2L, saved.rev)
        assertEquals(TaskStatus.DONE, session.state.value.tasks?.record("t-1")?.status)
    }

    @Test
    fun aConflictReloadsAndSaysWhatHappened() = runBlocking {
        val bridge = FakeBridge().apply {
            capabilities = listOf("task-records")
            tasks = listOf(record("t-1"))
        }
        val session = connected(bridge)
        val seen = session.state.value.tasks!!.records.single()
        // Another device changed it meanwhile.
        bridge.tasks = listOf(record("t-1", rev = 2, status = TaskStatus.REVIEW))
        try {
            session.updateTask(seen, TaskStatus.DONE)
            fail("a stale rev should fail")
        } catch (e: PaneSession.Failure.Bridge) {
            assertEquals(ErrorCode.CONFLICT, e.body.code)
            assertTrue(PaneSession.describe(e).contains("changed on another device"))
        }
        assertEquals(TaskStatus.REVIEW, session.state.value.tasks?.record("t-1")?.status)
    }
}
