package tech.xdge.muxa.demo

import kotlinx.coroutines.asCoroutineDispatcher
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import tech.xdge.muxa.session.earlierResults
import tech.xdge.muxa.session.currentResults
import tech.xdge.muxa.session.assistNote
import tech.xdge.muxa.session.RESULT_READY
import org.junit.Assert.assertNull
import tech.xdge.muxa.protocol.Delivery
import tech.xdge.muxa.protocol.AssistSpec
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
    private suspend fun connected(level: Int = WORK_ASSISTANT, pace: AssistPace = AssistPace.AT_ONCE): PaneSession {
        val session = PaneSession(Executors.newSingleThreadExecutor().asCoroutineDispatcher())
        session.connect { DemoBridge(workLevel = level, assistPace = pace) }
        return session
    }

    private fun PaneSession.request(id: String) = state.value.requests.first { it.id == id }

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

    /** A host below level 8 has no assistant: the request is refused, as before. */
    @Test
    fun askingTheAssistantBelowLevelEightIsUnsupported() = runBlocking {
        val bridge = DemoBridge(workLevel = WORK_WORKTREE_END)
        bridge.send(Wire.encode(Request.RequestAssistant(7, "w-0000b001", 2, AssistScope.RESEARCH)))
        val result = bridge.lines.receiveAsFlow().map { Wire.decodeMessage(it) }
            .first { it is Message.Result && it.id == 7L } as Message.Result
        assertEquals(ErrorCode.UNSUPPORTED, result.error?.code)
        bridge.close()
    }

    @Test
    fun theFixturesListARequestWithItsResultAndAnEarlierOne() = runBlocking {
        val session = connected()
        val q = session.request("q-0000a001")
        assertEquals(RESULT_READY, assistNote(session.work("w-0000b003"), session.state.value.requests))
        assertEquals(listOf(2L), q.currentResults.map { it.version })
        assertEquals(listOf(1L), q.earlierResults.map { it.version })
    }

    @Test
    fun getWorkReadsEveryCaptureText() = runBlocking {
        val session = connected()
        val detail = session.getWork("w-0000b001")
        assertEquals(listOf("Plan the team offsite\nTwo days in May, near the coast."), detail.captures.map { it.text })
        assertEquals("no Work item w-0000ffff", PaneSession.describe(refused { session.getWork("w-0000ffff") }))
    }

    /** Pending with a live session, then on the next list a Result with sources, and the session closes. */
    @Test
    fun aRequestRunsAndReturnsItsResultOnTheNextList() = runBlocking {
        val session = connected()
        val q = session.requestAssistant(session.work("w-0000b001"), AssistScope.RESEARCH, "Where to go")
        assertEquals(Delivery.PENDING, q.delivery)
        assertTrue(session.state.value.work.first { it.id == "w-0000b001" }.requests.contains(q.id))
        session.loadWork()
        val done = session.request(q.id)
        assertEquals(Delivery.RESULTED, done.delivery)
        val result = done.currentResults.single()
        assertTrue(result.findings.isNotEmpty() && result.findings.all { it.sources.isNotEmpty() })
        assertTrue(result.openQuestions.isNotEmpty() && result.next.isNotEmpty() && result.files.isNotEmpty())
        assertTrue(result.possibleDuplicates.single().reason.isNotEmpty())
        session.refresh()
        assertTrue(session.state.value.sessions.none { it.taskId == q.run })
    }

    @Test
    fun theSameRevReturnsTheSameRequestAndAskingAgainAddsAVersion() = runBlocking {
        val session = connected()
        val seen = session.work("w-0000b001")
        val first = session.requestAssistant(seen, AssistScope.RESEARCH)
        assertEquals(first.id, session.requestAssistant(seen, AssistScope.RESEARCH).id)
        session.loadWork()
        val again = session.requestAssistant(session.work("w-0000b001"), AssistScope.DRAFT)
        assertEquals(first.id, again.id)
        assertEquals(2L, again.version)
        session.loadWork()
        val q = session.request(first.id)
        assertEquals(listOf(2L), q.currentResults.map { it.version })
        assertEquals(listOf(1L), q.earlierResults.map { it.version })
    }

    @Test
    fun aSecondOpenRunInAWorkspaceIsRefusedWithTheLimit() = runBlocking {
        val session = connected(pace = AssistPace.NEVER)
        val q = session.requestAssistant(session.work("w-0000b001"), AssistScope.RESEARCH)
        session.refresh()
        assertTrue(session.state.value.sessions.any { it.taskId == q.run })
        assertEquals(WorkState.IN_PROGRESS, session.work("w-0000b001").state)
        val other = session.work("w-0000b003")
        val e = refused { session.requestAssistant(other, AssistScope.RESEARCH, workspace = "personal") }
        assertTrue(e is PaneSession.Failure.Refused)
        assertTrue(PaneSession.describe(e).contains("max_open_runs"))
    }

    @Test
    fun cancelIsRequestedThenCancelledAndTheRunsSessionCloses() = runBlocking {
        val session = connected(pace = AssistPace.NEVER)
        val q = session.requestAssistant(session.work("w-0000b001"), AssistScope.COMPARE)
        assertEquals(Delivery.CANCEL_REQUESTED, session.cancelRequest(q).delivery)
        session.loadWork()
        assertEquals(Delivery.CANCELLED, session.request(q.id).delivery)
        session.refresh()
        assertTrue(session.state.value.sessions.none { it.taskId == q.run })
        assertTrue(refused { session.cancelRequest(q.copy(version = 5)) } is PaneSession.Failure.Refused)
    }

    @Test
    fun anAssistedCaptureBecomesAWorkItemWithARequest() = runBlocking {
        val session = connected(pace = AssistPace.NEVER)
        val a = session.captureAndAsk("client-a", "Compare error trackers\nSentry or Rollbar.", "work", AssistSpec(AssistScope.COMPARE))
        assertNull(a.refusal)
        assertEquals("Compare error trackers", a.work?.title)
        assertEquals(AssistScope.COMPARE, a.request?.scope)
        assertTrue(session.state.value.captures.none { it.id == a.capture.id })
    }

    @Test
    fun anAssistedCaptureWithAnUnknownWorkspaceIsKeptUnsorted() = runBlocking {
        val session = connected()
        val a = session.captureAndAsk("client-b", "Idea", "nowhere", AssistSpec(AssistScope.RESEARCH))
        assertEquals(ErrorCode.BAD_REQUEST, (a.refusal as PaneSession.Failure.Bridge).body.code)
        assertTrue(session.state.value.captures.any { it.id == a.capture.id })
    }
}
