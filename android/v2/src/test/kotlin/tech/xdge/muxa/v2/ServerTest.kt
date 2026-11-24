package tech.xdge.muxa.v2

import kotlinx.coroutines.delay
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Assume.assumeTrue
import org.junit.Before
import org.junit.Test
import tech.xdge.muxa.transport.ProcessBytes
import java.io.File

/**
 * Against the real server on the fake backend: `make test-android-v2` sets
 * `muxa.v2server` to the built `fake_server` example.
 */
class ServerTest {
    private val program: String? = System.getProperty("muxa.v2server")
    private lateinit var server: Process
    private lateinit var socket: File

    @Before
    fun start() {
        assumeTrue("set MUXA_TEST_V2_SERVER", program != null)
        socket = File.createTempFile("muxa-v2-", ".sock").also { it.delete() }
        server = ProcessBuilder(program, socket.path).redirectOutput(ProcessBuilder.Redirect.DISCARD).start()
        repeat(100) { if (!socket.exists()) Thread.sleep(50) }
    }

    @After
    fun stop() {
        if (::server.isInitialized) server.destroy()
        if (::socket.isInitialized) socket.delete()
    }

    private suspend fun connect(state: V2Client.State = V2Client.State()) =
        V2Client.open(ProcessBytes("nc -U ${socket.path}"), "test", state)

    private suspend fun V2Client.next(skip: (V2Event) -> Boolean = { false }): V2Event {
        while (true) {
            val e = events.receive()
            if (!skip(e)) return e
        }
    }

    @Test
    fun attachShowsTheScreenAndTypes() = runBlocking {
        withTimeout(10_000) {
            val c = connect()
            val demo = c.terminals().first { it.name == "demo" }
            val ch = c.attach(demo.id, 80, 24)
            assertEquals(V2Event.Paint(ch), c.next())
            assertEquals(V2Event.Output(ch, "hello ".toByteArray()), c.next())
            c.input(ch, "x".toByteArray())
            assertEquals(V2Event.Output(ch, "echo:x".toByteArray()), c.next())
            c.close()
        }
    }

    @Test
    fun theReaderListsAndOpensTheFoldersFiles() = runBlocking {
        withTimeout(10_000) {
            val c = connect()
            val demo = c.terminals().first { it.name == "demo" }
            val changes = c.readerList(demo.id, ReaderKind.CHANGES)
            assertEquals("demo", changes.root)
            assertEquals(listOf(ReaderEntry("a.txt", status = "modified", added = 1, removed = 1)), changes.entries)
            assertEquals("@@ -1 +1 @@\n-one\n+two\n", c.readerOpen(demo.id, ReaderKind.CHANGES, "a.txt").second.text)
            val image = c.readerOpen(demo.id, ReaderKind.IMAGES, "shot.png").second
            assertEquals(listOf(0x89.toByte(), 0x50, 0x4E, 0x47), image.bytes?.toList())
            try {
                c.readerOpen(demo.id, ReaderKind.DOCS, "../secret.md")
                fail("only listed files open")
            } catch (e: V2Error.Refused) {
                assertEquals("bad-request", e.code)
            }
            c.close()
        }
    }

    @Test
    fun aTasksLoopStartsAndStops() = runBlocking {
        withTimeout(10_000) {
            val c = connect()
            val (agent, _) = c.startTask("w", "fix", loop = LoopSpec(check = "make test"))
            val started = c.loopStart(agent.id, reset = true)
            assertEquals(LoopInfo(agent.id, "make test", 3, 600, "armed", 0, 1, 1), started)
            assertEquals("user", c.loopStop(agent.id).reason)
            try {
                c.startTask("w", "bad", loop = LoopSpec(check = "rm -rf /"))
                fail("only an allowed check")
            } catch (e: V2Error.Refused) {
                assertEquals("bad-request", e.code)
            }
            c.close()
        }
    }

    @Test
    fun aTasksChatOpensWithItsPrompt() = runBlocking {
        withTimeout(10_000) {
            val c = connect()
            val (agent, _) = c.startTask("w", "chat")
            val (chat, update) = c.chatOpen(agent.id)
            assertEquals(listOf("Clean the build folder", "Removing it."), update.entries.map { it.text })
            assertEquals(listOf(ChatOption("Allow", "1"), ChatOption("Deny", "\u001b", cancel = true)), update.prompt?.options)
            c.chatClose(chat)
            c.close()
        }
    }

    @Test
    fun aResumeContinuesWithoutAPaint() = runBlocking {
        withTimeout(10_000) {
            val first = connect()
            val demo = first.terminals().first { it.name == "demo" }
            val ch = first.attach(demo.id, 80, 24)
            first.next()
            first.next()
            val state = first.state()
            first.close()
            delay(200)
            val again = connect(state)
            assertEquals(listOf(ch), again.resumed)
            again.input(ch, "y".toByteArray())
            // No paint and no `hello ` again: only the new output.
            assertEquals(V2Event.Output(ch, "echo:y".toByteArray()), again.next())
            again.close()
        }
    }

    @Test
    fun aCaptureReachesAWatchingDevice() = runBlocking {
        withTimeout(10_000) {
            val phone = connect()
            phone.watch()
            val mac = connect()
            val capture = mac.parseCapture("reply to the bank #personal due 12/1", "2026-11-25")
            assertEquals(Capture(ItemKind.REPLY, "to the bank", "Personal", "2026-12-01"), capture)
            val inbox = mac.inbox(V2Request.InboxAdd(capture))
            assertEquals(listOf("to the bank"), inbox.items.map { it.title })
            val pushed = phone.next { it !is V2Event.InboxChanged }
            assertEquals(V2Event.InboxChanged(inbox), pushed)
            try {
                mac.inbox(V2Request.InboxDone("nope"))
                fail("done on a missing item")
            } catch (e: V2Error.Refused) {
                assertEquals("not-found", e.code)
            }
            assertTrue(phone.resumed.isEmpty())
            phone.close()
            mac.close()
        }
    }

    @Test
    fun workspacesListTheHostsWorkspace() = runBlocking {
        withTimeout(10_000) {
            val c = connect()
            val expected = Workspace(
                "Personal", "/work", listOf("app", "docs"), agent = "claude",
                loop = WorkspaceLoop(LoopChecks.List(listOf("make test"))),
                connections = listOf(
                    WorkspaceConnection("github", isDefault = true, usedBy = UsedBy(routines = listOf("digest")), lastUsed = 1_700_000_000),
                ),
                mcp = listOf(WorkspaceMcp("docs", isDefault = true)),
                branches = mapOf("app" to "main"),
            )
            assertEquals(listOf(expected), c.workspaces())
            // Every Task agent uses every connection and server of its Workspace (ADR 0075).
            val (agent, _) = c.startTask("Personal", "uses")
            val after = c.workspaces().single()
            assertTrue(agent.id in after.connections[0].usedBy.tasks)
            assertTrue(agent.id in after.mcp[0].usedBy.tasks)
            c.close()
        }
    }

    @Test
    fun aReviewIsApprovedOrSentBackToItsAgent() = runBlocking {
        withTimeout(10_000) {
            val c = connect()
            val (agent, _) = c.startTask("w", "close")
            val review = c.inbox(V2Request.TaskReview(agent.id)).items.first { it.terminal == agent.id }
            assertEquals(ItemKind.REVIEW, review.kind)
            val approved = c.inbox(V2Request.InboxApprove(review.id))
            assertTrue(approved.done.any { it.id == review.id })
            assertTrue(c.terminals().none { it.id == agent.id })

            val (other, _) = c.startTask("w", "again")
            val second = c.inbox(V2Request.TaskReview(other.id)).items.first { it.terminal == other.id }
            val sent = c.inbox(V2Request.InboxRequestChanges(second.id, "Add a test."))
            assertTrue(sent.done.any { it.id == second.id })
            assertTrue(sent.items.none { it.id == second.id })
            c.close()
        }
    }

    @Test
    fun routinesListAndRunOnce() = runBlocking {
        withTimeout(10_000) {
            val c = connect()
            val (routines, problems, off) = c.routines()
            assertEquals(listOf("digest", "audit"), routines.map { it.name })
            assertEquals("09:00", routines[0].at)
            assertTrue(problems.isEmpty())
            assertEquals(listOf(RoutinesOff("lab", 2)), off)
            c.runRoutine("Personal", "audit")
            try {
                c.runRoutine("Personal", "digest")
                fail("digest already runs")
            } catch (e: V2Error.Refused) {
                assertEquals("conflict", e.code)
            }
            c.close()
        }
    }

    @Test
    fun subscriptionsComeWithTheirLimits() = runBlocking {
        withTimeout(10_000) {
            val c = connect()
            assertEquals(
                listOf(
                    SubscriptionUsage(
                        "claude-work", "claude-code", "Max 20x",
                        listOf(UsageWindow(300, 82, 1_764_014_400), UsageWindow(10_080, 31, 1_764_320_400)),
                        1_764_000_120,
                    ),
                ),
                c.subscriptions(),
            )
            c.close()
        }
    }

    @Test
    fun settingsShowTheFilesAndApplyOpsOnce() = runBlocking {
        withTimeout(10_000) {
            val c = connect()
            val expected = Settings(
                "1", readOnly = false, problem = null,
                workspaces = listOf(
                    SettingsWorkspace(
                        "Personal", "~/personal", listOf("~/src/app"),
                        foundRepos = listOf("app", "docs"), hiddenRepos = listOf("docs"), parkAfter = null,
                        agent = SettingsAgent("claude", listOf(EnvKey("CLAUDE_CONFIG_DIR", true))),
                        loop = WorkspaceLoop(LoopChecks.List(listOf("make test"))),
                        routinesEnabled = true, directWrites = false,
                        connections = listOf(SettingsConnection("github", "GITHUB_TOKEN", "me@example.com", "read", SecretState.SET)),
                        mcp = listOf(McpSpec("docs", command = "npx", args = listOf("-y", "docs-mcp"))),
                        routines = listOf(
                            SettingsRoutine(
                                "digest", at = "08:00", timezone = "Europe/Berlin", instructions = "routines/digest.md",
                                enabled = true, instructionsText = "Summarize the day.\n",
                            ),
                        ),
                    ),
                ),
                workers = listOf(WorkerSpec("reviewer", "Reviews diffs", "claude-code")),
                subscriptions = listOf(SubscriptionSpec("claude-work", "claude-code", "~/.claude-work", plan = "Max 20x")),
                foundLogins = listOf(FoundLogin("claude-code", workspace = "Personal")),
            )
            assertEquals(expected, c.settings())
            val ops = listOf(
                SettingsOp.EnvSet("Personal", "FOO", "bar"),
                SettingsOp.ConnectionAdd("Personal", ConnectionSpec("linear", "LINEAR_API_KEY"), "s3cret"),
            )
            val after = c.settingsUpdate("1", ops)
            assertEquals("2", after.revision)
            val ws = after.workspaces.single()
            assertEquals(listOf("CLAUDE_CONFIG_DIR", "FOO"), ws.agent.env.map { it.key })
            assertEquals(SecretState.SET, ws.connections.single { it.name == "linear" }.secret)
            val text = json.encodeToString(Settings.serializer(), after)
            assertTrue(text, "s3cret" !in text && "bar" !in text)
            try {
                c.settingsUpdate("1", ops)
                fail("a stale revision")
            } catch (e: V2Error.Refused) {
                assertEquals("conflict", e.code)
            }
            c.close()
        }
    }
}
