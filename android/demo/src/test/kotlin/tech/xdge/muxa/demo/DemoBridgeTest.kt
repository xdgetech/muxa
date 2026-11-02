package tech.xdge.muxa.demo

import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.asCoroutineDispatcher
import kotlinx.coroutines.delay
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test
import tech.xdge.muxa.protocol.AgentState
import tech.xdge.muxa.protocol.ChatPrompt
import tech.xdge.muxa.protocol.DiffScope
import tech.xdge.muxa.protocol.ErrorCode
import tech.xdge.muxa.protocol.LoopReason
import tech.xdge.muxa.protocol.LoopSpec
import tech.xdge.muxa.protocol.LoopState
import tech.xdge.muxa.protocol.RunReason
import tech.xdge.muxa.protocol.TaskStatus
import tech.xdge.muxa.protocol.WorkspaceChecks
import tech.xdge.muxa.session.InboxGroup
import tech.xdge.muxa.session.LoopDraft
import tech.xdge.muxa.session.TaskRecordDraft
import tech.xdge.muxa.session.inboxItem
import tech.xdge.muxa.session.inbox
import tech.xdge.muxa.session.PaneSession
import tech.xdge.muxa.terminal.PaneScreen
import java.io.ByteArrayInputStream
import java.util.concurrent.Executors
import javax.imageio.ImageIO

class DemoBridgeTest {
    private suspend fun connected(set: DemoBridge.Content = DemoBridge.Content.TESTS): Pair<PaneSession, PaneScreen> {
        val session = PaneSession(oneThread())
        val screen = PaneScreen()
        session.renderer = screen
        session.connect { DemoBridge(set) }
        return session to screen
    }

    private fun PaneScreen.settledRows(): List<String> {
        awaitIdle()
        return rows()
    }

    @Test
    fun listsTheSessions() = runBlocking {
        val (session, _) = connected()
        val s = session.state.value
        assertEquals("demo", s.tmuxVersion)
        assertTrue(s.canChat && s.canReadDocs && s.canShowChanges && s.canShowImages && s.canStartTasks && s.canCloseTasks)
        assertEquals(listOf("feature-auth", "fix-login", "docs-site", "scratch", "tall", "pair", "device"), s.sessions.map { it.name })
    }

    @Test
    fun theTasksFillEveryInboxGroup() = runBlocking {
        for (set in DemoBridge.Content.entries) {
            val (session, _) = connected(set)
            val s = session.state.value
            val groups = inbox(s.tasks?.records.orEmpty(), s.sessions).map { it.group }
            assertEquals(set.name, InboxGroup.entries, groups)
        }
    }

    @Test
    fun aCreatedTaskStartsARunningSession() = runBlocking {
        val (session, _) = connected()
        val draft = TaskRecordDraft(workspace = "personal", title = "Plan the trip!", goal = "Book it").request!!
        val saved = session.createTask(draft)
        assertEquals("t-new001", saved.id)
        assertEquals(InboxGroup.RUNNING, inboxItem(saved, session.state.value.sessions).group)
        assertEquals("plan-the-trip", session.state.value.sessions.first { it.taskId == saved.id }.name)
    }

    @Test
    fun aFailedLaunchKeepsTheTaskWithANote() = runBlocking {
        val (session, _) = connected()
        val draft = TaskRecordDraft(workspace = "work", title = "T", goal = "G", repoDir = "/src/app", branch = DemoBridge.FAILING_BRANCH)
        try {
            session.createTask(draft.request!!)
            fail("the launch should fail")
        } catch (e: PaneSession.Failure.LaunchFailed) {
            assertTrue(e.record.note!!.contains("already exists"))
        }
        val kept = session.state.value.tasks!!.records.last()
        assertEquals(InboxGroup.NO_AGENT, inboxItem(kept, session.state.value.sessions).group)
    }

    @Test
    fun aStatusChangeChecksTheRev() = runBlocking {
        val (session, _) = connected()
        val seen = session.state.value.tasks!!.record("t-login1")!!
        val done = session.updateTask(seen, TaskStatus.DONE)
        assertEquals(InboxGroup.CLOSED, inboxItem(done, session.state.value.sessions).group)
        try {
            session.updateTask(seen, TaskStatus.REVIEW)
            fail("a stale rev should fail")
        } catch (e: PaneSession.Failure.Bridge) {
            assertEquals(ErrorCode.CONFLICT, e.body.code)
        }
        assertEquals(TaskStatus.DONE, session.state.value.tasks!!.record("t-login1")!!.status)
    }

    @Test
    fun aTaskSessionListsItsBrief() = runBlocking {
        val (session, _) = connected()
        val listing = session.documents("%1")
        // The Task's loop ran a check, so its CHECK.md is listed too.
        val brief = listOf(DemoBridge.TASK_DOC, DemoBridge.HANDOFF_DOC, DemoBridge.CHECK_DOC)
        assertEquals(brief, listing.entries.takeLast(3).map { it.path })
        session.openDocument("%1", DemoBridge.TASK_DOC)
        waitUntil { session.state.value.doc?.text?.contains("passkey") == true }
        session.openDocument("%1", DemoBridge.HANDOFF_DOC)
        waitUntil { session.state.value.doc?.text?.contains("- Run 2 · claude-code · c9f0f895") == true }
        session.openDocument("%1", DemoBridge.CHECK_DOC)
        waitUntil { session.state.value.doc?.text?.contains("- Exit: 2") == true }
        // A session without a Task has no brief.
        assertTrue(session.documents("%3").entries.none { it.path == DemoBridge.TASK_DOC || it.path == DemoBridge.HANDOFF_DOC })
    }

    @Test
    fun theTasksShowEveryLoopState() = runBlocking {
        val (session, _) = connected()
        val s = session.state.value
        assertTrue(s.canLoop)
        val loops = s.tasks!!.records.mapNotNull { it.loop }
        assertEquals(
            setOf(LoopState.ARMED, LoopState.CHECKING, LoopState.PAUSED, LoopState.PASSED, LoopState.STOPPED),
            loops.map { it.state }.toSet(),
        )
        assertEquals(
            setOf(LoopReason.WAITING, LoopReason.LIMIT, LoopReason.TIMEOUT, LoopReason.USER),
            loops.mapNotNull { it.reason }.toSet(),
        )
        assertEquals(listOf(WorkspaceChecks.Any, null), s.tasks!!.workspaces.drop(1).map { it.checks })
    }

    @Test
    fun aLoopStartsAndStopsWithinTheWorkspacesChecks() = runBlocking {
        val (session, _) = connected()
        val deps = session.state.value.tasks!!.record("t-deps01")!!
        val started = session.startLoop(deps, LoopSpec("cargo test --workspace", 5))
        assertEquals(LoopState.ARMED, started.loop?.state)
        assertEquals(1, started.loop?.attempts)
        assertEquals(deps.rev + 1, started.rev)
        assertEquals(0, session.startLoop(started, LoopSpec("make check"), reset = true).loop?.attempts)
        val stopped = session.stopLoop(started)
        assertEquals(LoopReason.USER, stopped.loop?.reason)
        for ((task, check) in listOf("t-deps01" to "rm -rf /", "t-notes1" to "make check")) {
            try {
                session.startLoop(session.state.value.tasks!!.record(task)!!, LoopSpec(check))
                fail("$check is not allowed in $task's workspace")
            } catch (e: PaneSession.Failure.Bridge) {
                assertEquals(ErrorCode.BAD_REQUEST, e.body.code)
            }
        }
    }

    @Test
    fun aNewTaskStartsItsLoopAndAStatusChangeStopsIt() = runBlocking {
        val (session, _) = connected()
        val loop = LoopDraft(WorkspaceChecks.Any, "npm run lint")
        val saved = session.createTask(TaskRecordDraft(workspace = "personal", title = "Lint", goal = "G", loop = loop).request!!)
        assertEquals(LoopState.ARMED, saved.loop?.state)
        val review = session.updateTask(saved, TaskStatus.REVIEW)
        assertEquals(LoopReason.STATUS, review.loop?.reason)
    }

    @Test
    fun aRestartAddsARunAndTheAgentRunsAgain() = runBlocking {
        val (session, _) = connected()
        assertTrue(session.state.value.canRestartAgents)
        val login = session.state.value.tasks!!.record("t-login1")!!
        val saved = session.restartAgent(login, force = false)
        assertEquals(listOf(RunReason.START, RunReason.RESTART), saved.runs.map { it.reason })
        assertEquals(saved.runs[1].startedAt, saved.runs[0].endedAt)
        assertEquals(null, saved.runs[1].endedAt)
        waitUntil { inboxItem(saved, session.state.value.sessions).group == InboxGroup.RUNNING }
        // Running now: a second restart needs force.
        try {
            session.restartAgent(saved, force = false)
            fail("a running agent needs force")
        } catch (e: PaneSession.Failure.Bridge) {
            assertEquals(ErrorCode.BAD_REQUEST, e.body.code)
        }
        assertEquals(3L, session.restartAgent(saved, force = true).runs.last().id)
    }

    @Test
    fun aTaskWithoutASessionCannotRestart() = runBlocking {
        val (session, _) = connected()
        try {
            session.restartAgent(session.state.value.tasks!!.record("t-price1")!!, force = true)
            fail("no session runs it")
        } catch (e: PaneSession.Failure.Bridge) {
            assertEquals(ErrorCode.TASK_FAILED, e.body.code)
        }
    }

    @Test
    fun watchingRendersTheTestPattern() = runBlocking {
        val (session, screen) = connected()
        session.watch("%1")
        waitUntil { screen.settledRows().size == 30 }
        val rows = screen.rows()
        assertEquals("0123456789".repeat(10), rows[0])
        assertEquals("claude — pane %1 100x30", rows[1])
        assertEquals("日本語 ✓ é  red green blue", rows[3])
        assertEquals("$", rows[5])
        assertEquals(" ".repeat(99) + "X", rows[29])
    }

    @Test
    fun theDeviceSessionTakesTheViewportAndFixedPanesKeepTheirSize() = runBlocking {
        val (session, screen) = connected()
        session.setViewport(tech.xdge.muxa.session.PaneViewport(120, 40))
        session.watch("%9")
        waitUntil { screen.settledRows().getOrNull(1) == "zsh — pane %9 120x40" }
        // A rotation: the watched pane follows.
        session.setViewport(tech.xdge.muxa.session.PaneViewport(90, 50))
        waitUntil { screen.settledRows().getOrNull(1) == "zsh — pane %9 90x50" }
        session.watch("%1")
        waitUntil { screen.settledRows().getOrNull(1) == "claude — pane %1 100x30" }
    }

    @Test
    fun aFreshShellShowsATitleBarAndAPrompt() = runBlocking {
        val (session, screen) = connected()
        session.watch("%8")
        waitUntil { screen.settledRows().size == 2 }
        assertEquals("zsh — pane %8 100x60", screen.rows()[0])
        assertEquals("$", screen.rows()[1])
    }

    @Test
    fun typedTextEchoes() = runBlocking {
        val (session, screen) = connected()
        session.watch("%1")
        waitUntil { screen.settledRows().size == 30 }
        session.send("hi")
        waitUntil { screen.settledRows()[5] == "$ hi" }
        session.send(listOf("BSpace", "Enter"))
        waitUntil { screen.settledRows()[6] == "$" }
        assertEquals("$ h", screen.rows()[5])
    }

    @Test
    fun theChatOnTheAgentPaneAnswersAPrompt() = runBlocking {
        val (session, _) = connected()
        session.watch("%1")
        session.follow("%1")
        waitUntil { session.state.value.chat?.loaded == true }
        val chat = session.state.value.chat!!
        assertEquals("claude-code", chat.agent)
        assertEquals(listOf("u:1", "a:1", "t:1", "a:2", "t:2", "n:1"), chat.entries.map { it.id })

        session.paste("/allow")
        session.send(listOf("Enter"))
        waitUntil { session.state.value.chat?.prompt != null }
        val prompt = session.state.value.chat!!.prompt!!
        assertEquals(ChatPrompt.Kind.PERMISSION, prompt.kind)
        assertEquals(listOf("Allow", "Deny"), prompt.options.map { it.label })

        session.send(listOf("1"))
        waitUntil { session.state.value.chat?.prompt == null }
        assertEquals("Chose Allow", session.state.value.chat!!.entries.last().tool?.result)
    }

    @Test
    fun onlyTheAgentPaneHasAChat() = runBlocking {
        val (session, _) = connected()
        session.follow("%2")
        assertTrue("%2" in session.state.value.unsupportedChats)
    }

    @Test
    fun documentsListOpenAndUpdate() = runBlocking {
        val (session, _) = connected()
        val list = session.documents("%1")
        assertEquals(listOf("plans/demo-plan.md", "README.md", "docs/design.md", DemoBridge.TASK_DOC, DemoBridge.HANDOFF_DOC, DemoBridge.CHECK_DOC), list.entries.map { it.path })
        session.openDocument("%1", "README.md")
        waitUntil { session.state.value.doc?.text != null }
        assertTrue(session.state.value.doc!!.text!!.startsWith("# App\n"))

        session.openDocument("%1", "plans/demo-plan.md")
        waitUntil { session.state.value.doc?.text?.endsWith("Updated by the agent while you read.\n") == true }
        assertEquals(2L, session.state.value.doc!!.mtime)
        // Long enough to scroll, and not indented into a code block.
        assertTrue(session.state.value.doc!!.text!!.contains("\n40. A step of the plan.\n\nThe end of the plan.\n"))
    }

    @Test
    fun changesListAndOpenInBothScopes() = runBlocking {
        val (session, _) = connected()
        val list = session.changes("%1")
        assertEquals(listOf("src/parser.rs", "docs/notes.md", "assets/logo.png"), list.entries.map { it.path })
        assertEquals(listOf("src/item.rs"), list.branch?.entries?.map { it.path })

        session.openDiff("%1", "src/parser.rs")
        waitUntil { session.state.value.diff?.loaded == true }
        assertTrue(session.state.value.diff!!.text!!.startsWith("diff --git a/src/parser.rs b/src/parser.rs\n"))

        session.openDiff("%1", "src/item.rs", DiffScope.BRANCH)
        waitUntil { session.state.value.diff?.text?.contains("+#[derive(Debug, Clone, PartialEq)]\n") == true }

        session.openDiff("%1", "assets/logo.png")
        waitUntil { session.state.value.diff?.binary == true }
    }

    @Test
    fun imagesListAndOpenAsPng() = runBlocking {
        val (session, _) = connected()
        val list = session.images("%1")
        assertEquals(4, list.entries.size)
        session.openImage("%1", "docs/images/logo.png")
        waitUntil { session.state.value.image?.data != null }
        val png = ImageIO.read(ByteArrayInputStream(session.state.value.image!!.data!!))
        assertEquals(1024, png.width)
        assertEquals(768, png.height)
        assertEquals(0xFF9500, png.getRGB(0, 0) and 0xFFFFFF)
        assertEquals(0xFFFFFF, png.getRGB(512, 384) and 0xFFFFFF)
    }

    @Test
    fun aTakenBranchFailsAndAnotherStartsATask() = runBlocking {
        val (session, _) = connected()
        try {
            session.newTask("/src/app", DemoBridge.FAILING_BRANCH, null)
            fail("the task should fail")
        } catch (e: PaneSession.Failure.Bridge) {
            assertEquals(ErrorCode.TASK_FAILED, e.body.code)
        }
        assertEquals("feat-x", session.newTask("/src/app", "feat/x", null))
        val task = session.state.value.sessions.single { it.name == "feat-x" }
        assertEquals("app", task.repo)
        assertEquals("feat/x", task.branch)
    }

    @Test
    fun theShowcaseListsItsSessions() = runBlocking {
        val (session, _) = connected(DemoBridge.Content.SHOWCASE)
        assertEquals(
            listOf("stripe-webhooks", "retry-backoff", "dark-mode", "search-index", "notes"),
            session.state.value.sessions.map { it.name },
        )
        // The bridge's alerts make $4 wait.
        assertEquals("search-index", session.state.value.sessions.single { it.id == "$4" }.name)
    }

    @Test
    fun theShowcaseChatsOnTheAgentPaneAndWaits() = runBlocking {
        val (session, _) = connected(DemoBridge.Content.SHOWCASE)
        session.follow("%1")
        waitUntil { session.state.value.chat?.loaded == true }
        val chat = session.state.value.chat!!
        assertEquals(listOf("u:1", "a:1", "t:1", "t:2", "a:2", "t:3", "a:3"), chat.entries.map { it.id })
        assertEquals(AgentState.WAITING, chat.status?.state)
    }

    @Test
    fun theShowcaseReadsTheBillingRepo() = runBlocking {
        val (session, _) = connected(DemoBridge.Content.SHOWCASE)
        val docs = session.documents("%1")
        assertEquals("billing", docs.root)
        assertEquals(listOf("plans/stripe-webhooks.md", "README.md", "docs/webhooks.md", DemoBridge.TASK_DOC, DemoBridge.HANDOFF_DOC, DemoBridge.CHECK_DOC), docs.entries.map { it.path })
        val changes = session.changes("%1")
        assertEquals("billing", changes.root)
        assertEquals(listOf("src/webhooks/router.rs", "src/invoice.rs", "tests/webhooks.rs"), changes.entries.map { it.path })
    }

    @Test
    fun theShowcaseAgentScreenStartsWithThePrompt() = runBlocking {
        val (session, screen) = connected(DemoBridge.Content.SHOWCASE)
        session.watch("%1")
        val first = "> Handle Stripe's invoice.paid webhook. Mark the invoice paid and email a receipt."
        waitUntil { screen.settledRows().firstOrNull() == first }
    }

    private fun oneThread(): CoroutineDispatcher = Executors.newSingleThreadExecutor().asCoroutineDispatcher()

    private suspend fun waitUntil(timeoutMillis: Long = 5_000, condition: () -> Boolean) {
        val deadline = System.currentTimeMillis() + timeoutMillis
        while (!condition()) {
            if (System.currentTimeMillis() > deadline) throw AssertionError("condition not met in $timeoutMillis ms")
            delay(10)
        }
    }
}
