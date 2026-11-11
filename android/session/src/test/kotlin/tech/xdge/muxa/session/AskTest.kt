package tech.xdge.muxa.session

import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test
import tech.xdge.muxa.protocol.ErrorBody
import tech.xdge.muxa.protocol.ErrorCode
import tech.xdge.muxa.protocol.Message
import tech.xdge.muxa.protocol.Pane
import tech.xdge.muxa.protocol.Request
import tech.xdge.muxa.protocol.Role
import tech.xdge.muxa.protocol.Session
import tech.xdge.muxa.protocol.WorkspaceInfo

/** Workspace repos, a branch from the title and ask sessions (ADR 0031). */
class AskTest {
    /** Starts ask sessions as a `work:2` host does, or refuses with [refusal]. */
    private class AskBridge(private val refusal: ErrorBody? = null, private val named: Boolean = true) : FakeBridge() {
        override fun answer(req: Request) {
            if (req is Request.CloseTask) {
                if (sessions.none { it.id == req.session && it.ask != null }) {
                    return emit(Message.Result(req.id, error = ErrorBody(ErrorCode.BAD_REQUEST, "not an ask session")))
                }
                sessions = sessions.filter { it.id != req.session }
                return emit(Message.Result(req.id))
            }
            if (req !is Request.StartSession) return super.answer(req)
            if (refusal != null) return emit(Message.Result(req.id, error = refusal))
            val repo = req.repo.orEmpty()
            val name = "ask-${repo.substringAfterLast('/')}-1"
            sessions = sessions + Session(
                id = "$9", name = name, repo = repo.substringAfterLast('/'), repoDir = "/src/$repo", ask = req.workspace,
                panes = listOf(Pane("%9", "@9", 0, Role.AGENT, 80, 24, "claude", "/src/app", false, false)),
            )
            if (named) emit(Message.SessionStarted(req.id, "$9", name))
            emit(Message.Result(req.id))
        }
    }

    private suspend fun connected(bridge: FakeBridge, capabilities: List<String> = listOf("task-records", "work:2")): PaneSession {
        bridge.capabilities = capabilities
        val session = PaneSession(oneThread())
        session.connect {
            bridge.start()
            bridge
        }
        return session
    }

    @Test
    fun anAskSessionIsListedOnceStarted() = runBlocking {
        val bridge = AskBridge()
        val session = connected(bridge)
        assertTrue(session.state.value.canAsk)
        val started = session.startSession("work", "app")
        assertEquals("ask-app-1", started.name)
        assertEquals("$9", started.session)
        val sent = bridge.received.last { it is Request.StartSession } as Request.StartSession
        assertEquals("work" to "app", sent.workspace to sent.repo)
        val listed = session.state.value.sessions.single { it.id == "$9" }
        assertNull("an ask session runs no Task", listed.taskId)
        assertTrue(listed in session.state.value.sessions.manual)
    }

    @Test
    fun theHostsRefusalIsShown() = runBlocking {
        val session = connected(AskBridge(ErrorBody(ErrorCode.BAD_REQUEST, "nope is not a repo of work")))
        try {
            session.startSession("work", "nope")
            fail("the host refused")
        } catch (e: PaneSession.Failure.Bridge) {
            assertEquals("nope is not a repo of work", PaneSession.describe(e))
        }
    }

    @Test
    fun aResultWithoutTheSessionIsAnError() = runBlocking {
        val session = connected(AskBridge(named = false))
        try {
            session.startSession("work", "app")
            fail("no session was named")
        } catch (e: PaneSession.Failure.Bridge) {
            assertEquals(ErrorCode.INTERNAL, e.body.code)
        }
    }

    @Test
    fun anAskSessionCloses() = runBlocking {
        val bridge = AskBridge()
        val session = connected(bridge, listOf("close-task", "task-records", "work:5"))
        assertTrue(session.state.value.canCloseAskSessions)
        session.startSession("work", "app")
        val ask = session.state.value.sessions.single { it.id == "$9" }
        assertEquals("work", ask.ask)
        session.closeAskSession(ask)
        val sent = bridge.received.filterIsInstance<Request.CloseTask>().single()
        assertEquals("$9", sent.session)
        assertTrue("listed again", session.state.value.sessions.none { it.id == "$9" })
    }

    @Test
    fun aSessionWithoutAskOrAnOlderHostIsNeverClosed() = runBlocking {
        val older = AskBridge()
        val session = connected(older, listOf("close-task", "task-records", "work:4"))
        assertTrue(!session.state.value.canCloseAskSessions)
        session.startSession("work", "app")
        val newer = AskBridge()
        val current = connected(newer, listOf("close-task", "work:5"))
        for ((s, target) in listOf(
            session to session.state.value.sessions.single { it.id == "$9" },
            current to current.state.value.sessions.first { it.ask == null },
        )) {
            try {
                s.closeAskSession(target)
                fail("not closed")
            } catch (e: PaneSession.Failure.Bridge) {
                assertEquals(PaneSession.CLOSE_ASK_UNSUPPORTED, e.body.message)
            }
        }
        assertTrue((older.received + newer.received).none { it is Request.CloseTask })
    }

    @Test
    fun anOlderHostIsNeverAsked() = runBlocking {
        val bridge = AskBridge()
        val session = connected(bridge, listOf("task-records", "work:1"))
        assertTrue(!session.state.value.canAsk)
        try {
            session.startSession("work", "app")
            fail("work:1 has no ask sessions")
        } catch (e: PaneSession.Failure.Bridge) {
            assertEquals(ErrorCode.UNSUPPORTED, e.body.code)
            assertEquals(PaneSession.ASK_UNSUPPORTED, e.body.message)
        }
        assertTrue(bridge.received.none { it is Request.StartSession })
    }

    @Test
    fun reposShowRelativeToTheRootElseWithTheHomeFolderAsTilde() {
        assertEquals("app", repoDisplayName("/src/app", "/src"))
        assertEquals("fairpull/fairpull-plan", repoDisplayName("/w/fairpull/fairpull-plan", "/w/"))
        assertEquals("~/forks/tmux", repoDisplayName("/home/dev/forks/tmux", "/src"))
        assertEquals("~/code/x", repoDisplayName("/Users/dev/code/x", null))
        assertEquals("~/x", repoDisplayName("/root/x", null))
        assertEquals("/srcx/app", repoDisplayName("/srcx/app", "/src"))
        assertEquals("/opt/sdk", repoDisplayName("/opt/sdk", null))
    }

    @Test
    fun choicesComeFromTheWorkspaceElseFromTheSessionsOnAnOlderHost() {
        val work = WorkspaceInfo("work", repos = listOf("/home/dev/forks/tmux", "/src/app"), root = "/src")
        val notes = WorkspaceInfo("notes")
        val sessions = listOf(Session(id = "$1", name = "s", repo = "docs", repoDir = "/src/docs", panes = emptyList()))
        assertEquals(
            listOf(RepoChoice("/home/dev/forks/tmux", "~/forks/tmux"), RepoChoice("/src/app", "app")),
            repoChoices(work, listOf(work, notes), sessions),
        )
        assertEquals(emptyList<RepoChoice>(), repoChoices(notes, listOf(work, notes), sessions))
        assertEquals(listOf(RepoChoice("/src/docs", "docs")), repoChoices(notes, listOf(notes), sessions))
        assertEquals("work", listOf(notes, work).containing("/src/app")?.name)
        assertEquals("work", listOf(notes, work).containing("/src/new/repo")?.name)
        assertNull(listOf(notes, work).containing("/elsewhere"))
    }

    @Test
    fun aRepoTaskNeedsABranchUnlessTheHostNamesIt() {
        val draft = TaskRecordDraft(workspace = "work", title = "Add search", goal = "G", repoDir = "/src/app")
        assertEquals("Name the branch.", draft.problem)
        val named = draft.copy(branchFromTitle = true)
        assertNull(named.problem)
        assertNull("no branch is sent", named.request?.branch)
        assertEquals("feat/x", named.copy(branch = " feat/x ").request?.branch)
        assertEquals("a bad name is still refused", true, named.copy(branch = "a..b").problem != null)
    }
}
