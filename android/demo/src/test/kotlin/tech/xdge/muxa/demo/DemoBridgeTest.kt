package tech.xdge.muxa.demo

import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.asCoroutineDispatcher
import kotlinx.coroutines.delay
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test
import tech.xdge.muxa.protocol.ChatPrompt
import tech.xdge.muxa.protocol.DiffScope
import tech.xdge.muxa.protocol.ErrorCode
import tech.xdge.muxa.session.PaneSession
import tech.xdge.muxa.terminal.PaneScreen
import java.io.ByteArrayInputStream
import java.util.concurrent.Executors
import javax.imageio.ImageIO

class DemoBridgeTest {
    private suspend fun connected(): Pair<PaneSession, PaneScreen> {
        val session = PaneSession(oneThread())
        val screen = PaneScreen()
        session.renderer = screen
        session.connect { DemoBridge() }
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
        assertEquals(listOf("feature-auth", "fix-login", "docs-site", "scratch", "tall", "pair"), s.sessions.map { it.name })
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
        assertEquals(listOf("plans/demo-plan.md", "README.md", "docs/design.md"), list.entries.map { it.path })
        session.openDocument("%1", "README.md")
        waitUntil { session.state.value.doc?.text != null }
        assertTrue(session.state.value.doc!!.text!!.startsWith("# App\n"))

        session.openDocument("%1", "plans/demo-plan.md")
        waitUntil { session.state.value.doc?.text?.endsWith("Updated by the agent while you read.\n") == true }
        assertEquals(2L, session.state.value.doc!!.mtime)
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

    private fun oneThread(): CoroutineDispatcher = Executors.newSingleThreadExecutor().asCoroutineDispatcher()

    private suspend fun waitUntil(timeoutMillis: Long = 5_000, condition: () -> Boolean) {
        val deadline = System.currentTimeMillis() + timeoutMillis
        while (!condition()) {
            if (System.currentTimeMillis() > deadline) throw AssertionError("condition not met in $timeoutMillis ms")
            delay(10)
        }
    }
}
