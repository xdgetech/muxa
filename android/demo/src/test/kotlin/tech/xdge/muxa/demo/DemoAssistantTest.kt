package tech.xdge.muxa.demo

import kotlinx.coroutines.asCoroutineDispatcher
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.receiveAsFlow
import tech.xdge.muxa.protocol.AssistScope
import tech.xdge.muxa.protocol.ErrorCode
import tech.xdge.muxa.protocol.Message
import tech.xdge.muxa.protocol.Request
import tech.xdge.muxa.protocol.Wire
import tech.xdge.muxa.protocol.SortAction
import tech.xdge.muxa.protocol.WORK_ASSISTANT
import tech.xdge.muxa.protocol.WORK_WORKTREE_END
import tech.xdge.muxa.protocol.WorkAction
import tech.xdge.muxa.protocol.WorkState
import tech.xdge.muxa.session.PaneSession
import tech.xdge.muxa.session.TaskRecordDraft
import tech.xdge.muxa.session.forWork
import tech.xdge.muxa.session.listed
import java.util.concurrent.Executors

/** The demo's Captures and Work items at `work` level 8, as the UI tests use them (ADR 0046). */
class DemoAssistantTest {
    private suspend fun connected(level: Int = WORK_ASSISTANT): PaneSession {
        val session = PaneSession(Executors.newSingleThreadExecutor().asCoroutineDispatcher())
        session.connect { DemoBridge(workLevel = level) }
        return session
    }

    private fun PaneSession.work(id: String) = state.value.work.first { it.id == id }

    private suspend fun refused(block: suspend () -> Unit): PaneSession.Failure {
        try {
            block()
        } catch (e: PaneSession.Failure) {
            return e
        }
        fail("expected a refusal")
        throw IllegalStateException()
    }

    @Test
    fun theInboxListsOpenWorkThenLaterThenClosedAndTheUnsortedCaptures() = runBlocking {
        val s = connected().state.value
        assertEquals(listOf("w-0000b001", "w-0000b002", "w-0000b003", "w-0000b004"), s.work.map { it.id })
        // t-auth01 has an open Inbox item: the host derives the state.
        assertEquals(WorkState.WAITING_DECISION, s.work[1].state)
        assertEquals(listOf("w-0000b001", "w-0000b002", "w-0000b003"), s.work.listed().map { it.id })
        assertEquals(listOf("c-0000c001", "c-0000c002"), s.captures.map { it.id })
    }

    @Test
    fun anOlderLevelListsNoWorkAndRefusesACapture() = runBlocking {
        val session = connected(WORK_WORKTREE_END)
        assertTrue(session.state.value.work.isEmpty())
        refused { session.capture("id-1", "Idea", null) }
        Unit
    }

    @Test
    fun aResentCaptureIsTheSameCapture() = runBlocking {
        val session = connected()
        val first = session.capture("client-1", "Book the venue", "personal")
        val again = session.capture("client-1", "Book the venue", "personal")
        assertEquals(first.id, again.id)
        assertEquals(first.id, session.state.value.captures.first().id)
    }

    @Test
    fun laterMovesAnItemBelowTheOthersAndCompleteClosesIt() = runBlocking {
        val session = connected()
        session.updateWork(session.work("w-0000b001"), WorkAction.LATER)
        assertEquals(listOf("w-0000b002", "w-0000b003", "w-0000b001"), session.state.value.work.listed().map { it.id })
        session.updateWork(session.work("w-0000b002"), WorkAction.COMPLETE)
        assertEquals(listOf("w-0000b003", "w-0000b001"), session.state.value.work.listed().map { it.id })
    }

    @Test
    fun aStaleRevIsAConflict() = runBlocking {
        val session = connected()
        val seen = session.work("w-0000b001")
        session.updateWork(seen, WorkAction.LATER)
        assertTrue(refused { session.updateWork(seen, WorkAction.COMPLETE) } is PaneSession.Failure.WorkChanged)
    }

    @Test
    fun sortingMakesAWorkItemFromTheFirstLineOrKeepsANote() = runBlocking {
        val session = connected()
        val flaky = session.state.value.captures.first { it.id == "c-0000c001" }
        val sorted = session.sortCapture(flaky, SortAction.WORK)
        val made = session.state.value.work.first { it.id == sorted.work.single() }
        assertEquals("Look into the flaky upload test", made.title)
        session.sortCapture(session.state.value.captures.single(), SortAction.NOTE)
        assertTrue(session.state.value.captures.isEmpty())
    }

    @Test
    fun aTaskForAWorkItemIsMadeOncePerRev() = runBlocking {
        val session = connected()
        session.loadTasks()
        val item = session.work("w-0000b001")
        val draft = TaskRecordDraft.forWork(item, session.state.value.tasks?.workspaces.orEmpty())
        val first = session.createTask(draft.request!!)
        val again = session.createTask(draft.request!!)
        assertEquals(first.id, again.id)
        assertEquals("w-0000b001", first.work)
        assertEquals(listOf(first.id), session.work("w-0000b001").tasks)
    }

    @Test
    fun askingTheAssistantIsUnsupported() = runBlocking {
        val bridge = DemoBridge(workLevel = WORK_ASSISTANT)
        bridge.send(Wire.encode(Request.RequestAssistant(7, "w-0000b001", 2, AssistScope.RESEARCH)))
        val result = bridge.lines.receiveAsFlow().map { Wire.decodeMessage(it) }
            .first { it is Message.Result && it.id == 7L } as Message.Result
        assertEquals(ErrorCode.UNSUPPORTED, result.error?.code)
        bridge.close()
    }
}
