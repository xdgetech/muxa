package tech.xdge.muxa.session

import kotlinx.coroutines.runBlocking
import org.junit.Assert.*
import org.junit.Test
import tech.xdge.muxa.protocol.*

class WorkerTest {
    @Test fun scopesAndLabelsDoNotGuessALegacyRunsWorker() {
        assertTrue(WorkerInfo("All", "codex").allows("work"))
        assertFalse(WorkerInfo("None", "codex", workspaces = emptyList()).allows("work"))
        assertTrue(WorkerInfo("One", "codex", workspaces = listOf("work")).allows("work"))
        assertEquals("Workspace default", workerLabel(DEFAULT_WORKER))
        assertEquals("Reviewer", workerLabel("Reviewer"))
        assertEquals("Worker unrecorded", workerLabel(null))
        val workers = listOf(WorkerInfo("Reviewer", "codex"))
        assertEquals(workers, TaskBoard.of(Message.TaskList(1, emptyList(), emptyList(), workers = workers)).workers)
    }

    @Test fun restartSendsDisplayedRevisionForEverySelection() = runBlocking {
        val bridge = FakeBridge().apply {
            capabilities = listOf("task-records", "task-handoff", "workers")
            tasks = listOf(record())
            workers = listOf(WorkerInfo("Reviewer", "codex", workspaces = listOf("work")))
        }
        val session = PaneSession(oneThread())
        session.connect { bridge.start(); bridge }
        val displayed = record()
        for (selection in listOf(null, DEFAULT_WORKER, "Reviewer")) {
            session.restartAgent(displayed, false, selection)
            val request = bridge.received.filterIsInstance<Request.RestartAgent>().last()
            assertEquals(selection, request.worker)
            assertEquals(displayed.rev, request.rev)
        }
    }

    @Test fun createCarriesWorkerSelection() = runBlocking {
        val bridge = FakeBridge().apply {
            capabilities = listOf("task-records", "workers")
            workers = listOf(WorkerInfo("Reviewer", "codex", workspaces = listOf("work")))
        }
        val session = PaneSession(oneThread())
        session.connect { bridge.start(); bridge }
        val draft = TaskRecordDraft(workspace = "work", title = "Title", goal = "Goal", worker = "Reviewer")
        session.createTask(draft.request!!)
        assertEquals("Reviewer", bridge.received.filterIsInstance<Request.CreateTask>().last().worker)
    }

    @Test fun oldHostsNeverReceiveWorkerIntent() = runBlocking {
        val bridge = FakeBridge().apply {
            capabilities = listOf("task-records", "task-handoff")
            tasks = listOf(record())
            workers = listOf(WorkerInfo("Reviewer", "codex", workspaces = listOf("work")))
        }
        val session = PaneSession(oneThread())
        session.connect { bridge.start(); bridge }
        try {
            session.restartAgent(record(), false, DEFAULT_WORKER)
            fail("worker selection must be refused")
        } catch (e: PaneSession.Failure.Bridge) {
            assertEquals(ErrorCode.UNSUPPORTED, e.body.code)
        }
        val draft = TaskRecordDraft(workspace = "work", title = "Title", goal = "Goal", worker = "Reviewer")
        try {
            session.createTask(draft.request!!)
            fail("worker create must be refused")
        } catch (e: PaneSession.Failure.Bridge) {
            assertEquals(ErrorCode.UNSUPPORTED, e.body.code)
        }
        assertTrue(bridge.received.none { it is Request.RestartAgent || it is Request.CreateTask })
    }

    @Test fun staleWorkerMetadataPreventsSubmittingAnOpenSheet() = runBlocking {
        val bridge = FakeBridge().apply {
            capabilities = listOf("task-records", "task-handoff", "workers")
            workers = listOf(WorkerInfo("Reviewer", "codex", workspaces = listOf("work")))
            tasks = listOf(record())
        }
        val session = PaneSession(oneThread())
        session.connect { bridge.start(); bridge }
        val draft = TaskRecordDraft(workspace = "work", title = "Title", goal = "Goal", worker = "Reviewer")
        bridge.workers = listOf(WorkerInfo("Reviewer", "codex", workspaces = listOf("other")))
        session.loadTasks()
        try {
            session.createTask(draft.request!!)
            fail("changed scope must invalidate the retained selection")
        } catch (e: PaneSession.Failure.Bridge) { assertEquals(ErrorCode.BAD_REQUEST, e.body.code) }
        bridge.workers = emptyList()
        session.loadTasks()
        try {
            session.restartAgent(record(), false, "Reviewer")
            fail("removed worker must not be sent")
        } catch (e: PaneSession.Failure.Bridge) { assertEquals(ErrorCode.BAD_REQUEST, e.body.code) }
        assertTrue(bridge.received.none { it is Request.CreateTask || it is Request.RestartAgent })
        assertNull(workerProblem(null, "other", emptyList()))
        assertNull(workerProblem(DEFAULT_WORKER, "other", emptyList()))
        assertNotNull(workerProblem("Reviewer", "other", listOf(WorkerInfo("Reviewer", "codex", workspaces = listOf("work")))))
    }

    private fun record() = TaskRecord("t-1", 7, "work", "Title", "Goal", TaskStatus.OPEN, 1, 1, session = "s")
}
