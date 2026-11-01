package tech.xdge.muxa.session

import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
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
}
