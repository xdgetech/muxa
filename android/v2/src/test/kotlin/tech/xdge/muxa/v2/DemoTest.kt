package tech.xdge.muxa.v2

import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test

/** The client against the in-memory demo server. */
class DemoTest {
    private suspend fun open() = V2Client.open(DemoChannel(), "test")

    @Test
    fun aTerminalShowsItsScreenAndEchoes() = runBlocking {
        withTimeout(10_000) {
            val c = open()
            val shell = c.terminals().first { it.name == "shell" }
            val ch = c.attach(shell.id, 80, 24)
            assertEquals(V2Event.Paint(ch), c.events.receive())
            assertEquals(V2Event.Output(ch, "$ ".toByteArray()), c.events.receive())
            c.input(ch, "ls\r".toByteArray())
            assertEquals(V2Event.Output(ch, "ls\r\n".toByteArray()), c.events.receive())
            c.close()
        }
    }

    @Test
    fun aCaptureGoesOnTopAndWatchersSeeIt() = runBlocking {
        withTimeout(10_000) {
            val c = open()
            c.watch()
            val capture = c.parseCapture("reply Sam #personal due tomorrow", "2026-11-25")
            assertEquals(Capture(ItemKind.REPLY, "Sam", "Personal", "2026-11-26"), capture)
            val inbox = c.inbox(V2Request.InboxAdd(capture))
            assertEquals("Sam", inbox.items.first().title)
            assertEquals(V2Event.InboxChanged(inbox), c.events.receive())
            val done = c.inbox(V2Request.InboxDone(inbox.items[0].id))
            assertEquals("Sam", done.done.first().title)
            c.close()
        }
    }

    @Test
    fun aRunningRoutineIsAConflict() = runBlocking {
        withTimeout(10_000) {
            val c = open()
            val (routines, _) = c.routines()
            assertEquals(listOf("dependency-digest", "nightly-backup", "release-notes"), routines.map { it.name })
            c.runRoutine("app", "release-notes")
            try {
                c.runRoutine("app", "release-notes")
                fail("release-notes already runs")
            } catch (e: V2Error.Refused) {
                assertEquals("conflict", e.code)
            }
            c.close()
        }
    }

    @Test
    fun aTaskStartsInItsPlaceOnce() = runBlocking {
        withTimeout(10_000) {
            val c = open()
            val (audit, existing) = c.startTask("app", "Audit", places = listOf(PlaceSpec.Read("/demo/app")))
            assertEquals(Triple("Audit", false, null), Triple(audit.name, existing, audit.branch))
            val fix = listOf(PlaceSpec.Branch("/demo/app", "fix-2"))
            val (first, _) = c.startTask("app", "Fix", places = fix)
            assertEquals("fix-2", first.branch)
            val (again, running) = c.startTask("app", "Fix", places = fix)
            assertEquals(first.id to true, again.id to running)
            val (looped, _) = c.startTask("app", "Lint", loop = LoopSpec("make lint", 5))
            assertEquals(listOf(Triple(looped.id, "make lint", 5)), c.loops().map { Triple(it.terminal, it.check, it.maxAttempts) })
            c.close()
        }
    }

    @Test
    fun placesDecodeFromTheLabel() {
        val t = Terminal(
            "t9", "Audit", live = true, cols = 80, rows = 24,
            labels = mapOf("muxa.places" to """[{"kind":"folder","path":"/w/tasks/a"},{"kind":"read","path":"/w/docs"}]"""),
        )
        assertEquals(listOf(Place(PlaceKind.FOLDER, "/w/tasks/a"), Place(PlaceKind.READ, "/w/docs")), t.places)
    }

    @Test
    fun aTasksFolderHasAReader() = runBlocking {
        withTimeout(10_000) {
            val c = open()
            val terminals = c.terminals()
            val agent = terminals.first { it.isTaskAgent }
            assertEquals(listOf("PLAN.md", ".muxa/TASK.md"), c.readerList(agent.id, ReaderKind.DOCS).entries.map { it.path })
            val (reader, plan) = c.readerOpen(agent.id, ReaderKind.DOCS, "PLAN.md")
            assertEquals(true, plan.text?.startsWith("# Plan"))
            // The plan links another listed document, for the Reader's links.
            assertEquals(true, plan.text?.contains("](.muxa/TASK.md)"))
            c.readerClose(reader)
            assertEquals("modified", c.readerList(agent.id, ReaderKind.CHANGES).entries.first().status)
            val image = c.readerOpen(agent.id, ReaderKind.IMAGES, "screenshots/login.png").second
            assertEquals(listOf(0x89.toByte(), 0x50, 0x4E, 0x47), image.bytes?.take(4))
            val shell = terminals.first { it.name == "shell" }
            try {
                c.readerList(shell.id, ReaderKind.DOCS)
                fail("a shell has no folder")
            } catch (e: V2Error.Refused) {
                assertEquals("bad-request", e.code)
            }
            c.close()
        }
    }

    @Test
    fun aLoopStartsAndStopsAndWatchersSeeIt() = runBlocking {
        withTimeout(10_000) {
            val c = open()
            c.watch()
            val started = c.loopStart("t1", LoopSpec(maxAttempts = 5))
            assertEquals("armed" to 5, started.state to started.maxAttempts)
            assertEquals(V2Event.Loops(listOf(started)), c.events.receive())
            val stopped = c.loopStop("t1")
            assertEquals("stopped" to "user", stopped.state to stopped.reason)
            assertEquals(listOf(stopped), c.loops())
            try {
                c.loopStart("t3")
                fail("a shell has no loop")
            } catch (e: V2Error.Refused) {
                assertEquals("bad-request", e.code)
            }
            c.close()
        }
    }

    @Test
    fun aTasksChatShowsItsConversationAndPrompt() = runBlocking {
        withTimeout(10_000) {
            val c = open()
            val (_, update) = c.chatOpen("t2")
            assertEquals(true, update.reset)
            assertEquals(listOf("user", "assistant", "tool"), update.entries.map { it.kind })
            assertEquals(listOf("Allow", "Deny"), update.prompt?.options?.map { it.label })
            try {
                c.chatOpen("t3")
                fail("a shell has no chat")
            } catch (e: V2Error.Refused) {
                assertEquals("bad-request", e.code)
            }
            c.close()
        }
    }

    @Test
    fun workspacesListWhatATaskMayUse() = runBlocking {
        withTimeout(10_000) {
            val c = open()
            val (app, personal) = c.workspaces()
            assertEquals(listOf("app", "docs"), app.repos)
            assertEquals(LoopChecks.List(listOf("make test", "make lint")), app.loop?.checks)
            assertEquals(listOf("github", "linear"), app.connections.filter { it.isDefault }.map { it.name })
            assertEquals(Triple("Personal", "claude", null), Triple(personal.name, personal.agent, personal.loop))
            c.close()
        }
    }

    @Test
    fun approveClosesTheTaskAndResolvesTheReview() = runBlocking {
        withTimeout(10_000) {
            val c = open()
            c.watch()
            val review = c.inbox().items.first { it.kind == ItemKind.REVIEW }
            assertEquals("t1", review.terminal)
            val inbox = c.inbox(V2Request.InboxApprove(review.id))
            assertEquals(V2Event.Removed("t1"), c.events.receive())
            assertEquals(V2Event.InboxChanged(inbox), c.events.receive())
            assertEquals(review.id, inbox.done.first().id)
            assertEquals(false, c.terminals().any { it.id == "t1" })
            c.close()
        }
    }

    @Test
    fun approveAsksForForceWhenTheWorktreeHoldsWork() = runBlocking {
        withTimeout(10_000) {
            val c = open()
            try {
                c.inbox(V2Request.InboxApprove("d8"))
                fail("d8's worktree holds work")
            } catch (e: V2Error.Refused) {
                assertEquals("would-lose-work", e.code)
                assertTrue(e.message.orEmpty().contains("uncommitted changes"))
            }
            assertTrue("nothing closed", c.terminals().any { it.id == "t2" })
            assertTrue(c.inbox().items.any { it.id == "d8" })
            val forced = c.inbox(V2Request.InboxApprove("d8", WorktreeChoice.FORCE))
            assertEquals("d8", forced.done.first().id)
            assertTrue(c.terminals().none { it.id == "t2" })
            c.close()
        }
    }

    @Test
    fun workspacesSayWhoUsesEachConnectionAndServer() = runBlocking {
        withTimeout(10_000) {
            val c = open()
            val app = c.workspaces().first()
            val every = listOf("dependency-digest", "nightly-backup", "release-notes")
            assertEquals(UsedBy(every, listOf("t1", "t2")), app.connections[0].usedBy)
            assertEquals(1_764_000_000L, app.connections[0].lastUsed)
            assertEquals(UsedBy(every, listOf("t1", "t2")), app.connections[1].usedBy)
            // A named connection is ignored: every agent gets all of them (ADR 0075).
            val (t, _) = c.startTask("app", "Triage", connections = listOf("linear"))
            assertEquals("""["github","linear"]""", t.labels["muxa.connections"])
            assertEquals("""["docs"]""", t.labels["muxa.mcp"])
            val after = c.workspaces().first()
            assertEquals(listOf("t1", "t2", t.id), after.connections[1].usedBy.tasks)
            assertEquals(listOf("t1", "t2", t.id), after.mcp[0].usedBy.tasks)
            c.close()
        }
    }

    @Test
    fun requestChangesTypesTheCommentToTheAgent() = runBlocking {
        withTimeout(10_000) {
            val c = open()
            try {
                c.inbox(V2Request.InboxRequestChanges("d1", " "))
                fail("an empty comment")
            } catch (e: V2Error.Refused) {
                assertEquals("bad-request", e.code)
            }
            val inbox = c.inbox(V2Request.InboxRequestChanges("d1", "Add a test."))
            assertEquals("d1", inbox.done.first().id)
            val ch = c.attach("t1", 80, 24)
            assertEquals(V2Event.Paint(ch), c.events.receive())
            val screen = (c.events.receive() as V2Event.Output).bytes.toString(Charsets.UTF_8)
            assertEquals(true, screen.endsWith("Add a test.\r\n"))
            c.close()
        }
    }

    @Test
    fun retryRunsTheRoutineOfAFailedItem() = runBlocking {
        withTimeout(10_000) {
            val c = open()
            val failed = c.inbox().items.first { it.kind == ItemKind.FAILED }
            assertEquals(RoutineRef("app", "nightly-backup"), failed.routine)
            assertEquals(failed.id, c.inbox(V2Request.InboxRetry(failed.id)).done.first().id)
            assertNotNull(c.routines().first.first { it.name == "nightly-backup" }.runningSince)
            try {
                c.runRoutine("app", "nightly-backup")
                fail("nightly-backup already runs")
            } catch (e: V2Error.Refused) {
                assertEquals("conflict", e.code)
            }
            c.close()
        }
    }

    @Test
    fun onlyAFailedItemIsRetried() = runBlocking {
        withTimeout(10_000) {
            val c = open()
            for ((item, code) in listOf("d1" to "bad-request", "nope" to "not-found")) {
                try {
                    c.inbox(V2Request.InboxRetry(item))
                    fail("retry of $item")
                } catch (e: V2Error.Refused) {
                    assertEquals(code, e.code)
                }
            }
            c.close()
        }
    }

    @Test
    fun itemsDecodeWithAndWithoutTheirTaskOrRoutine() {
        val old = json.decodeFromString<InboxItem>("""{"id":"a","kind":"todo","title":"x","created":1,"source":"capture"}""")
        assertEquals(null to null, old.terminal to old.routine)
        val linked = json.decodeFromString<InboxItem>(
            """{"id":"b","kind":"failed","title":"y","created":1,"source":"routine","terminal":"t9","routine":{"workspace":"w","name":"r"}}""",
        )
        assertEquals("t9" to RoutineRef("w", "r"), linked.terminal to linked.routine)
    }

    @Test
    fun aWorkspaceLoopIsNoneAnyOrAList() {
        fun ws(loop: String) = json.decodeFromString<Workspace>(
            """{"name":"w","root":"/w","repos":[],"loop":$loop,"connections":[{"name":"github","default":true}]}""",
        )
        assertEquals(null, ws("null").loop)
        assertEquals(LoopChecks.Any, ws("""{"checks":"any"}""").loop?.checks)
        assertEquals(LoopChecks.List(listOf("make test")), ws("""{"checks":["make test"]}""").loop?.checks)
        assertEquals(WorkspaceConnection("github", isDefault = true), ws("null").connections.single())
        val back = json.decodeFromString<WorkspaceLoop>(json.encodeToString(WorkspaceLoop(LoopChecks.Any)))
        assertEquals(LoopChecks.Any, back.checks)
    }

    @Test
    fun anUpdateReplacesEntriesById() {
        val before = listOf(ChatEntry("a", "user", "1"), ChatEntry("b", "tool"))
        val update = ChatUpdate(entries = listOf(ChatEntry("b", "tool", "done"), ChatEntry("c", "notice")))
        assertEquals(listOf("a", "b", "c"), update.appliedTo(before).map { it.id })
        assertEquals("done", update.appliedTo(before)[1].text)
        assertEquals(emptyList<ChatEntry>(), ChatUpdate(reset = true, entries = emptyList()).appliedTo(before))
    }

    @Test
    fun theDemoShowsItsSettings() = runBlocking {
        withTimeout(10_000) {
            val c = open()
            val s = c.settings()
            assertEquals("1", s.revision)
            assertEquals(listOf("app", "Personal"), s.workspaces.map { it.name })
            val app = s.workspaces[0]
            assertEquals(SecretState.SET, app.connections.single().secret)
            assertEquals("docs", app.mcp.single().name)
            assertTrue(app.routines.single().instructionsText.orEmpty().isNotEmpty())
            assertEquals(listOf("reviewer"), s.workers.map { it.name })
            c.close()
        }
    }

    @Test
    fun settingsOpsApplyAndKeepOnlyThatASecretIsSet() = runBlocking {
        withTimeout(10_000) {
            val c = open()
            val s = c.settingsUpdate(
                "1",
                listOf(
                    SettingsOp.EnvSet("app", "AAA", "bar-value"),
                    SettingsOp.ConnectionAdd("app", ConnectionSpec("linear", "LINEAR_API_KEY"), "s3cret"),
                    SettingsOp.ConnectionAdd("app", ConnectionSpec("notion", "NOTION_TOKEN")),
                    SettingsOp.WorkspaceUpdate("app", agentCommand = Patch.Clear, parkAfter = Patch.Set("2d")),
                    SettingsOp.WorkspaceRemove("Personal"),
                ),
            )
            assertEquals("2", s.revision)
            val app = s.workspaces.single()
            assertEquals(listOf(EnvKey("AAA", true), EnvKey("CLAUDE_CONFIG_DIR", true)), app.agent.env)
            assertEquals(listOf(SecretState.SET, SecretState.SET, SecretState.MISSING), app.connections.map { it.secret })
            assertEquals(null to "2d", app.agent.command to app.parkAfter)
            val text = json.encodeToString(Settings.serializer(), s)
            assertTrue(text, "s3cret" !in text && "bar-value" !in text)
            assertEquals(s, c.settings())
            c.close()
        }
    }

    @Test
    fun aStaleRevisionIsAConflictAndAFailedOpChangesNothing() = runBlocking {
        withTimeout(10_000) {
            val c = open()
            val before = c.settings()
            val bad = refusal {
                c.settingsUpdate("1", listOf(SettingsOp.EnvSet("app", "X", "y"), SettingsOp.ConnectionRemove("app", "nope")))
            }
            assertEquals("bad-request", bad.code)
            assertTrue(bad.message, bad.message.orEmpty().startsWith("ops[1] connection-remove: "))
            assertEquals(before, c.settings())
            val exists = refusal { c.settingsUpdate("1", listOf(SettingsOp.McpAdd("app", McpSpec("docs", command = "x")))) }
            assertTrue(exists.message, exists.message.orEmpty().startsWith("ops[0] mcp-add: "))
            c.settingsUpdate("1", listOf(SettingsOp.EnvUnset("app", "CLAUDE_CONFIG_DIR")))
            assertEquals("conflict", refusal { c.settingsUpdate("1", listOf(SettingsOp.WorkerRemove("reviewer"))) }.code)
            c.close()
        }
    }

    private suspend fun refusal(block: suspend () -> Unit): V2Error.Refused {
        try {
            block()
        } catch (e: V2Error.Refused) {
            return e
        }
        throw AssertionError("not refused")
    }
}
