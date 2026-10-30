package tech.xdge.muxa.session

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import tech.xdge.muxa.protocol.Agent
import tech.xdge.muxa.protocol.AgentState
import tech.xdge.muxa.protocol.ImageInfo
import tech.xdge.muxa.protocol.Pane
import tech.xdge.muxa.protocol.Role
import tech.xdge.muxa.protocol.Session

/** Session lists, image folders, alerts and task drafts. */
class ModelTest {
    private fun pane(id: String, role: Role, window: String = "@1", shell: Boolean = false, state: AgentState? = null) =
        Pane(id, window, 0, role, 80, 24, if (shell) "zsh" else "claude", "/", false, shell, state?.let { Agent(it) })

    @Test
    fun taggedRolesToggle() {
        val s = Session("$1", "s", panes = listOf(pane("%1", Role.HIDDEN, shell = true), pane("%2", Role.AGENT), pane("%3", Role.SHELL, shell = true)))
        assertEquals("%2", s.defaultPane?.id)
        assertEquals("%3", s.counterpart("%2")?.id)
        assertEquals("%2", s.counterpart("%3")?.id)
    }

    @Test
    fun untaggedPanesUseTheShellInTheSameWindowAndNoCounterpartDisablesTheToggle() {
        val s = Session(
            "$1", "s",
            panes = listOf(pane("%1", Role.UNKNOWN), pane("%2", Role.UNKNOWN, shell = true), pane("%3", Role.UNKNOWN, "@2", shell = true)),
        )
        assertEquals("%2", s.counterpart("%1")?.id)
        assertEquals("%1", s.counterpart("%2")?.id)
        assertNull(Session("$1", "s", panes = listOf(pane("%1", Role.AGENT))).counterpart("%1"))
    }

    @Test
    fun theLastPaneIsReopenedWhileItsSessionIsThere() {
        val sessions = listOf(
            Session("$1", "a", panes = listOf(pane("%1", Role.AGENT), pane("%2", Role.SHELL, shell = true))),
            Session("$2", "b", panes = listOf(pane("%3", Role.AGENT), pane("%4", Role.HIDDEN))),
        )
        assertEquals("%2", sessions.resuming(LastPane("a", "%2"))?.id)
        // Gone or hidden: the session's default pane.
        assertEquals("%1", sessions.resuming(LastPane("a", "%9"))?.id)
        assertEquals("%3", sessions.resuming(LastPane("b", "%4"))?.id)
        // After a server restart the id can be another session's pane.
        assertEquals("%3", sessions.resuming(LastPane("b", "%2"))?.id)
        assertNull(sessions.resuming(LastPane("gone", "%1")))
    }

    @Test
    fun groupsByRepoInFirstSeenOrderWithUntaggedLast() {
        fun s(name: String, repo: String?) = Session("$$name", name, repo = repo, panes = emptyList())
        val groups = listOf(s("a1", "app"), s("x", null), s("b1", "api"), s("a2", "app"), s("y", ""), s("b2", "api")).groupedByRepo()
        assertEquals(listOf("app", "api", null), groups.map { it.repo })
        assertEquals(listOf(listOf("a1", "a2"), listOf("b1", "b2"), listOf("x", "y")), groups.map { g -> g.sessions.map { it.name } })
    }

    private val images = listOf(
        "build/screenshots/en/login.png", "build/screenshots/en/home.png", "build/screenshots/ja/login.png",
        "docs/images/chat.jpg", "logo.png",
    ).map { ImageInfo(it) }

    @Test
    fun foldersNestAndSingleChildChainsJoin() {
        val root = ImageFolder.tree(images)
        assertEquals(listOf("logo.png"), root.images.map { it.path })
        assertEquals(listOf("build/screenshots", "docs/images"), root.folders.map { it.name })
        val shots = root.folders[0]
        assertEquals("build/screenshots", shots.path)
        assertEquals(listOf("en", "ja"), shots.folders.map { it.name })
        assertEquals(listOf("build/screenshots/en/home.png", "build/screenshots/en/login.png"), shots.folders[0].images.map { it.path })
        assertEquals(3, shots.count)
        assertEquals(5, root.count)
    }

    @Test
    fun siblingsAreTheFoldersImagesInNameOrder() {
        assertEquals(
            listOf("build/screenshots/en/home.png", "build/screenshots/en/login.png"),
            ImageFolder.siblings("build/screenshots/en/login.png", images).map { it.path },
        )
        assertEquals(listOf("logo.png"), ImageFolder.siblings("logo.png", images).map { it.path })
        assertEquals(listOf("build", "build/screenshots", "build/screenshots/en"), ImageFolder.ancestors("build/screenshots/en/login.png"))
    }

    private fun agentSession(id: String, pane: String, state: AgentState?, shell: Boolean = false) =
        Session(id, "s$id", panes = listOf(pane(pane, Role.AGENT, shell = shell, state = state)))

    @Test
    fun attentionStatesAreTold() {
        val old = listOf(agentSession("$1", "%1", AgentState.RUNNING), agentSession("$2", "%2", AgentState.RUNNING), agentSession("$3", "%3", AgentState.IDLE))
        val new = listOf(agentSession("$1", "%1", AgentState.WAITING), agentSession("$2", "%2", AgentState.ERROR), agentSession("$3", "%3", AgentState.DONE))
        val alerts = new.alerts(old, null)
        assertEquals(listOf(AgentAlert.Kind.WAITING, AgentAlert.Kind.FAILED, AgentAlert.Kind.DONE), alerts.map { it.kind })
        assertEquals("needs your input", alerts.first().kind.message)
    }

    @Test
    fun aTurnEndingInIdleIsDoneAndQuietCasesAreNotTold() {
        val running = listOf(agentSession("$1", "%1", AgentState.RUNNING))
        assertEquals(listOf(AgentAlert.Kind.DONE), listOf(agentSession("$1", "%1", AgentState.IDLE)).alerts(running, null).map { it.kind })
        assertTrue(running.alerts(listOf(agentSession("$1", "%1", AgentState.IDLE)), null).isEmpty())

        val waiting = listOf(agentSession("$1", "%1", AgentState.WAITING))
        // Unchanged, the shown session, a new pane (the first list), and a stale state.
        assertTrue(waiting.alerts(waiting, null).isEmpty())
        assertTrue(waiting.alerts(running, "$1").isEmpty())
        assertTrue(waiting.alerts(emptyList(), null).isEmpty())
        assertTrue(listOf(agentSession("$1", "%1", AgentState.WAITING, shell = true)).alerts(running, null).isEmpty())
    }

    @Test
    fun aCompleteDraftBecomesARequest() {
        val req = TaskDraft(" /src/app ", "feat/search ", "").request
        assertEquals(TaskDraft.NewTask("/src/app", "feat/search", null), req)
        assertEquals("origin/main", TaskDraft("/src/app", "x", "origin/main").request?.base)
    }

    @Test
    fun draftMistakesAreNamedBeforeSending() {
        assertEquals("Pick a repository.", TaskDraft(branch = "x").problem)
        assertEquals("The repository path must start with /.", TaskDraft("src/app", "x").problem)
        assertEquals("Name the branch.", TaskDraft("/a", " ").problem)
        for (bad in listOf("-x", "a b", "a..b", "a:b")) assertNotNull(bad, TaskDraft("/a", bad).problem)
        assertNotNull(TaskDraft("/a", "x", "--all").problem)
        assertNull(TaskDraft("/a", "-x").request)
    }

    @Test
    fun knownReposAreListedOnceInOrder() {
        val sessions = listOf(
            Session("$1", "a", repo = "app", repoDir = "/src/app", panes = emptyList()),
            Session("$2", "b", repo = "docs", repoDir = "/src/docs", panes = emptyList()),
            Session("$3", "c", repo = "app", repoDir = "/src/app", panes = emptyList()),
            Session("$4", "d", panes = emptyList()),
        )
        assertEquals(listOf(KnownRepo("app", "/src/app"), KnownRepo("docs", "/src/docs")), sessions.knownRepos)
    }

    @Test
    fun fitKeepsTheFontReadable() {
        assertEquals(PaneFit.WHOLE, PaneFit.automatic(wide = false, wholeFont = 12f, widthFont = 14f))
        assertEquals(PaneFit.WIDTH, PaneFit.automatic(wide = true, wholeFont = 12f, widthFont = 14f))
        assertEquals(PaneFit.WIDTH, PaneFit.automatic(wide = false, wholeFont = 6f, widthFont = 10f))
        assertEquals(PaneFit.READABLE, PaneFit.automatic(wide = false, wholeFont = 4f, widthFont = 6f))
        assertEquals(9f, PaneFit.READABLE.fontSize(4f, 6f))
        assertEquals(PaneFit.WHOLE, PaneFit.pinched(0.5f))
        assertEquals(PaneFit.READABLE, PaneFit.pinched(2f))
        assertNull(PaneFit.pinched(1f))
    }
}
