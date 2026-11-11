package tech.xdge.muxa.demo

import kotlinx.coroutines.asCoroutineDispatcher
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test
import tech.xdge.muxa.protocol.CloseMode
import tech.xdge.muxa.protocol.ErrorCode
import tech.xdge.muxa.protocol.WORK_ASK_ROOT
import tech.xdge.muxa.protocol.WORK_ITEMS
import tech.xdge.muxa.protocol.WORK_MEMORY
import tech.xdge.muxa.protocol.WORK_ROUTINES
import tech.xdge.muxa.session.PaneSession
import tech.xdge.muxa.session.TaskRecordDraft
import tech.xdge.muxa.session.manual
import tech.xdge.muxa.session.repoChoices
import java.util.concurrent.Executors

/** The demo as a `work:2` host, and as an older one (ADR 0031). */
class DemoAskTest {
    private suspend fun connected(workLevel: Int? = null): PaneSession {
        val session = PaneSession(Executors.newSingleThreadExecutor().asCoroutineDispatcher())
        session.connect { if (workLevel == null) DemoBridge() else DemoBridge(workLevel = workLevel) }
        return session
    }

    @Test
    fun theWorkspaceListsItsReposByRelativeName() = runBlocking {
        val s = connected().state.value
        val workspaces = s.tasks?.workspaces.orEmpty()
        val names = repoChoices(workspaces.first { it.name == "work" }, workspaces, s.sessions).map { it.name }
        assertEquals(listOf("~/forks/tmux", "app", "docs", "libs/core"), names)
    }

    @Test
    fun anAskSessionStartsInTheMainCheckoutAsAManualSession() = runBlocking {
        val session = connected()
        assertTrue(session.state.value.canAsk)
        val first = session.startSession("work", "/src/app")
        assertEquals("ask-app-1", first.name)
        val second = session.startSession("work", "app")
        assertEquals("a relative repo works too", "ask-app-2", second.name)
        val listed = session.state.value.sessions.single { it.id == first.session }
        assertEquals("/src/app", listed.repoDir)
        assertNull(listed.worktree)
        assertTrue(listed in session.state.value.sessions.manual)
    }

    @Test
    fun anAskSessionClosesAndAManualSessionDoesNot() = runBlocking {
        val session = connected()
        assertTrue(session.state.value.canCloseAskSessions)
        val started = session.startSession("work", "app")
        val ask = session.state.value.sessions.single { it.id == started.session }
        assertEquals("work", ask.ask)
        session.closeAskSession(ask)
        assertTrue(session.state.value.sessions.none { it.id == started.session })
        val manual = session.state.value.sessions.first { it.worktree == null && it.ask == null }
        try {
            session.closeTask(manual.id, CloseMode.KEEP)
            fail("a manual session is the user's")
        } catch (e: PaneSession.Failure.Bridge) {
            assertEquals(ErrorCode.BAD_REQUEST, e.body.code)
        }
        assertTrue(session.state.value.sessions.any { it.id == manual.id })
    }

    @Test
    fun anOlderHostCannotCloseAnAskSession() = runBlocking {
        assertFalse(connected(WORK_ITEMS).state.value.canCloseAskSessions)
    }

    @Test
    fun aRepoOfNoWorkspaceIsRefused() = runBlocking {
        val session = connected()
        try {
            session.startSession("work", "/elsewhere/repo")
            fail("not a repo of work")
        } catch (e: PaneSession.Failure.Bridge) {
            assertEquals(ErrorCode.BAD_REQUEST, e.body.code)
            assertEquals("/elsewhere/repo is not a repo of work", e.body.message)
        }
    }

    @Test
    fun aRepoTaskWithoutABranchIsNamedAfterItsTitle() = runBlocking {
        val session = connected()
        val draft = TaskRecordDraft(workspace = "work", title = "Add search", goal = "G", repoDir = "/src/app", branchFromTitle = true)
        val record = session.createTask(draft.request!!)
        assertEquals("add-search", record.branch)
    }

    @Test
    fun anOlderHostNeedsTheBranchAndHasNoAsk() = runBlocking {
        val session = connected(WORK_ROUTINES)
        assertFalse(session.state.value.canAsk)
        val draft = TaskRecordDraft(workspace = "work", title = "Add search", goal = "G", repoDir = "/src/app", branchFromTitle = true)
        try {
            session.createTask(draft.request!!)
            fail("work:1 needs a branch")
        } catch (e: PaneSession.Failure.Bridge) {
            assertEquals(ErrorCode.BAD_REQUEST, e.body.code)
        }
    }

    @Test
    fun talkToAssistantOpensTheSecretarysOneSession() = runBlocking {
        val session = connected(WORK_ASK_ROOT)
        assertEquals("secretary", session.state.value.assistantWorkspace)
        val first = session.talkToAssistant()
        assertEquals("ask-secretary", first.name)
        assertEquals("the same conversation", first.session, session.talkToAssistant().session)
        assertEquals("secretary", session.state.value.sessions.single { it.id == first.session }.ask)
        assertNull("a level 14 demo has no Talk", connected(WORK_MEMORY).state.value.assistantWorkspace)
    }
}
