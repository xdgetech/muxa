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
    fun turnedOffRoutinesAreReportedUntilTurnedOn() = runBlocking {
        withTimeout(10_000) {
            val c = open()
            assertEquals(listOf(RoutinesOff("Personal", 1)), c.routines().third)
            c.settingsUpdate(c.settings().revision, listOf(SettingsOp.RoutinesGate("Personal", true)))
            assertTrue(c.routines().third.isEmpty())
        }
    }

    @Test
    fun aRunningRoutineIsAConflict() = runBlocking {
        withTimeout(10_000) {
            val c = open()
            val (routines, _) = c.routines()
            assertEquals(listOf("dependency-digest", "nightly-backup", "release-notes", "scan"), routines.map { it.name })
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
    fun aV1ItemResumesInItsPlaceOnce() = runBlocking {
        withTimeout(10_000) {
            val c = open()
            val item = c.inbox().items.first { it.id == "d9" }
            val place = checkNotNull(item.place)
            assertEquals(ItemPlace("app", "/demo/tasks/tidy-the-release-notes-t-1a2b3c"), place)
            val existing = listOf(PlaceSpec.Existing(place.path))
            val (agent, running) = c.startTask(place.workspace, "Tidy the release notes", places = existing)
            assertEquals(Triple("Tidy the release notes", false, place.path), Triple(agent.name, running, agent.taskDir))
            val (again, found) = c.startTask(place.workspace, "Tidy the release notes", places = existing)
            assertEquals(agent.id to true, again.id to found)
            c.close()
        }
    }

    @Test
    fun aCheckoutRunsOneAgentInTheReposOwnFolder() = runBlocking {
        withTimeout(10_000) {
            val c = open()
            val checkout = listOf(PlaceSpec.Checkout("app"))
            val (agent, running) = c.startTask("app", "app main", places = checkout)
            assertEquals(Triple(false, "/demo/app", "app"), Triple(running, agent.taskDir, agent.labels["muxa.checkout"]))
            val (again, found) = c.startTask("app", "app main", places = checkout)
            assertEquals(agent.id to true, again.id to found)
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
            assertEquals("the Task's shell goes with it", V2Event.Removed("t6"), c.events.receive())
            assertEquals(V2Event.InboxChanged(inbox), c.events.receive())
            assertEquals(review.id, inbox.done.first().id)
            assertEquals(false, c.terminals().any { it.id == "t1" })
            c.close()
        }
    }

    @Test
    fun anUploadGoesToTheTasksUploadsFolder() = runBlocking {
        withTimeout(10_000) {
            val c = open()
            assertEquals(".muxa/uploads/build.log", c.upload("log".toByteArray(), "build.log", "t1"))
            assertEquals(".muxa/uploads/build 2.log", c.upload("log".toByteArray(), "build.log", "t1"))
            try {
                c.upload(ByteArray(0), "../x", "t1")
                fail("only a file name")
            } catch (e: V2Error.Refused) {
                assertEquals("bad-request", e.code)
            }
            c.close()
        }
    }

    @Test
    fun aTasksShellsStartAndCloseAndCloseTaskTakesThemAll() = runBlocking {
        withTimeout(10_000) {
            val c = open()
            val shell = c.taskShell("t1")
            assertEquals(true, shell.isTaskShell)
            assertEquals("/demo/app-fix-login", shell.taskDir)
            c.taskShell("t1")
            try {
                c.taskShell("t1")
                fail("three shells per Task")
            } catch (e: V2Error.Refused) {
                assertEquals("conflict", e.code)
            }
            try {
                c.closeTerminal("t1")
                fail("the agent never closes this way")
            } catch (e: V2Error.Refused) {
                assertEquals("bad-request", e.code)
            }
            c.closeTerminal(shell.id)
            assertEquals(false, c.terminals().any { it.id == shell.id })
            val inbox = c.inbox(V2Request.CloseTask("t1"))
            assertEquals("d1", inbox.done.first().id)
            assertEquals(true, c.terminals().none { it.taskDir == "/demo/app-fix-login" })
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
            // t7, the talk with pr-watch, is a Task agent and gets them too (ADR 0083).
            assertEquals(UsedBy(every, listOf("t1", "t2", "t7")), app.connections[0].usedBy)
            assertEquals(1_764_000_000L, app.connections[0].lastUsed)
            assertEquals(UsedBy(every, listOf("t1", "t2", "t7")), app.connections[1].usedBy)
            // A named connection is ignored: every agent gets all of them (ADR 0075).
            val (t, _) = c.startTask("app", "Triage", connections = listOf("linear"))
            assertEquals("""["github","linear"]""", t.labels["muxa.connections"])
            assertEquals("""["docs"]""", t.labels["muxa.mcp"])
            val after = c.workspaces().first()
            assertEquals(listOf("t1", "t2", "t7", t.id), after.connections[1].usedBy.tasks)
            assertEquals(listOf("t1", "t2", "t7", t.id), after.mcp[0].usedBy.tasks)
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
    fun aBotsItemsCarryLinksAndAProposalAndCountWhenDone() = runBlocking {
        withTimeout(10_000) {
            val c = open()
            val inbox = c.inbox()
            val decide = inbox.items.first { it.id == "d10" }
            assertEquals("pr-watch", decide.bot)
            assertEquals("https://github.com/example/app/pull/57", decide.links.single().url)
            assertEquals("example/app#57", decide.proposal?.target)
            // The host's day and its count of items against daily_items come with the counts (ADR 0086).
            val today = java.time.LocalDate.now().toString()
            assertEquals(listOf(BotCount("app", "pr-watch", 4, 1, today, 2)), inbox.bots)
            assertEquals("pr-watch", c.routines().first.first { it.name == "scan" }.bot)
            assertEquals(listOf(BotCount("app", "pr-watch", 5, 1, today, 2)), c.inbox(V2Request.InboxDone("d10")).bots)
            assertEquals(listOf(BotCount("app", "pr-watch", 5, 2, today, 2)), c.inbox(V2Request.InboxDelete("d11")).bots)
            c.close()
        }
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
            assertTrue(app.routines.all { it.instructionsText.orEmpty().isNotEmpty() })
            val bot = app.bots.single()
            assertEquals("pr-watch", bot.name)
            assertEquals(2, bot.memoryVersions)
            assertEquals("pr-watch", app.routines.first { it.name == "scan" }.bot)
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

    @Test
    fun applyingABotsSettingsProposalChangesTheSettingsAndEndsTheItem() = runBlocking {
        withTimeout(10_000) {
            val c = open()
            val proposal = c.inbox().items.first { it.id == "d12" }.settingsProposal
            assertEquals(listOf("routine-add", "bot-update"), proposal?.ops?.map { it.op })
            val inbox = c.inbox(V2Request.InboxApply("d12"))
            assertTrue(inbox.items.none { it.id == "d12" })
            assertEquals("d12", inbox.done.first().id)
            val app = c.settings().workspaces.first { it.name == "app" }
            val sweep = app.routines.first { it.name == "nightly-sweep" }
            assertEquals(false to "pr-watch", sweep.enabled to sweep.bot)
            assertEquals(4, app.bots.single().dailyItems)
            c.close()
        }
    }

    @Test
    fun aProposalAfterTheSettingsChangedIsAConflictAndStaysOpen() = runBlocking {
        withTimeout(10_000) {
            val c = open()
            c.settingsUpdate("1", listOf(SettingsOp.RoutinesGate("Personal", true)))
            val conflict = refusal { c.inbox(V2Request.InboxApply("d12")) }
            assertEquals("conflict", conflict.code)
            assertTrue(conflict.message, "ask it again" in conflict.message.orEmpty())
            assertTrue(c.inbox().items.any { it.id == "d12" })
            assertTrue(c.settings().workspaces.first { it.name == "app" }.routines.none { it.name == "nightly-sweep" })
            c.close()
        }
    }

    @Test
    fun anAcceptedSuggestionChangesTheItemAndADismissedOneOnlyGoes() = runBlocking {
        withTimeout(10_000) {
            val c = open()
            val suggested = c.inbox().items.first { it.id == "d13" }
            assertEquals("pr-watch", suggested.suggestion?.bot)
            val accepted = c.inbox(V2Request.InboxSuggestion("d13", accept = true)).items.first { it.id == "d13" }
            assertEquals(Triple("app", "2026-11-28", "It failed twice on CI this week."), Triple(accepted.workspace, accepted.due, accepted.note))
            assertEquals(null, accepted.suggestion)
            assertEquals("bad-request", refusal { c.inbox(V2Request.InboxSuggestion("d13", accept = false)) }.code)
            c.close()
        }
        withTimeout(10_000) {
            val c = open()
            val dismissed = c.inbox(V2Request.InboxSuggestion("d13", accept = false)).items.first { it.id == "d13" }
            assertEquals(null to null, dismissed.workspace to dismissed.suggestion)
            c.close()
        }
    }

    @Test
    fun anAcceptedSplitPutsItsPartsInTheItemsPlace() = runBlocking {
        withTimeout(10_000) {
            val item = InboxItem(
                "s1", ItemKind.TODO, "Plan the trip", note = "In May.", created = 1, source = "capture",
                suggestion = ItemSuggestion(workspace = "Personal", split = listOf("Book the flights", "Book a hotel")),
            )
            val first = InboxItem("s0", ItemKind.READ, "First", created = 1, source = "capture")
            val c = V2Client.open(DemoChannel(DemoContent.STANDARD.copy(inbox = listOf(first, item))), "test")
            val inbox = c.inbox(V2Request.InboxSuggestion("s1", accept = true))
            assertEquals(listOf("First", "Book the flights", "Book a hotel"), inbox.items.map { it.title })
            assertEquals(listOf("Personal", "Personal"), inbox.items.drop(1).map { it.workspace })
            assertEquals("s1" to null, inbox.done.first().id to inbox.done.first().suggestion)
            c.close()
        }
    }

    @Test
    fun aTalkWithABotIsATaskWithItsLabel() = runBlocking {
        withTimeout(10_000) {
            val c = open()
            val (talk, existing) = c.startTask("app", "Talk to pr-watch", bot = "pr-watch")
            assertEquals(Triple("pr-watch", false, true), Triple(talk.bot, existing, talk.isTaskAgent))
            assertEquals(null, c.startTask("app", "Fix", places = listOf(PlaceSpec.Folder)).first.bot)
            assertEquals("bad-request", refusal { c.startTask("app", "Talk", bot = "PR Watch") }.code)
            c.close()
        }
    }

    @Test
    fun theCaptureAssistantsSettingsAreSaved() = runBlocking {
        withTimeout(10_000) {
            val c = open()
            val before = c.settings().assistant
            assertEquals(true, before?.enabled)
            val s = c.settingsUpdate("1", listOf(SettingsOp.AssistantUpdate(false, "reviewer", "Be brief.\n")))
            assertEquals(SettingsAssistant(false, "reviewer", "Be brief.\n", before?.memoryText), s.assistant)
            val kept = c.settingsUpdate(s.revision, listOf(SettingsOp.AssistantUpdate(true)))
            assertEquals(Triple(true, null, "Be brief.\n"), Triple(kept.assistant?.enabled, kept.assistant?.worker, kept.assistant?.soulText))
            c.close()
        }
    }

    /** ADR 0091: each subscription with its last report; one never used has none. */
    @Test
    fun subscriptionsShowTheirLastReport() = runBlocking {
        withTimeout(10_000) {
            val c = open()
            val usage = c.subscriptions()
            assertEquals(listOf("claude-personal", "claude-spare", "chatgpt"), usage.map { it.name })
            assertEquals(listOf(82, 31), usage[0].windows.map { it.usedPercent })
            assertEquals(false, usage[0].windows.any { it.hasReset() })
            assertEquals(Pair(emptyList<UsageWindow>(), null), Pair(usage[1].windows, usage[1].asOf))
            c.close()
        }
    }

    /** ADR 0091 §6: today's tokens, whose Task folders are the demo agents'. */
    @Test
    fun tokenUsageNamesTheTasksFolders() = runBlocking {
        withTimeout(10_000) {
            val c = open()
            val usage = c.tokenUsage()
            assertEquals(listOf("app", "Personal"), usage.workspaces.map { it.workspace })
            assertEquals("/demo/app-fix-login", usage.tasks.first().dir)
            assertEquals("2026-11-01", c.tokenUsage("2026-11-01").day)
            c.close()
        }
    }

    /** ADR 0091 §7: onboarding adopts app's env login, and a login terminal opens once per subscription. */
    @Test
    fun aFoundLoginIsAdoptedAndItsLoginTerminalOpensOnce() = runBlocking {
        withTimeout(10_000) {
            val c = open()
            assertEquals(listOf(FoundLogin("claude-code", workspace = "app")), c.settings().foundLogins)
            val s = c.settingsUpdate("1", listOf(SettingsOp.SubscriptionAdopt("app", "claude-code", "claude-app")))
            assertEquals(listOf("claude-app"), s.workspaces[0].subscriptions)
            assertEquals(false, s.workspaces[0].agent.env.any { it.key == "CLAUDE_CONFIG_DIR" })
            assertEquals(emptyList<FoundLogin>(), s.foundLogins)
            val (t, existing) = c.loginStart("claude-app")
            assertEquals(Triple(false, "login", "claude-app"), Triple(existing, t.labels["muxa.kind"], t.labels["muxa.subscription"]))
            val (again, open) = c.loginStart("claude-app")
            assertEquals(true to t.id, open to again.id)
            assertEquals("bad-request", refusal { c.loginStart("nope") }.code)
            c.close()
        }
    }

    /** ADR 0091: a login is named once and Workspaces list it; one a Workspace lists cannot go. */
    @Test
    fun subscriptionsAreAddedListedAndKeptWhileListed() = runBlocking {
        withTimeout(10_000) {
            val c = open()
            val before = c.settings()
            assertEquals(listOf("claude-personal", "claude-spare", "chatgpt"), before.subscriptions.map { it.name })
            assertEquals(listOf("claude-personal", "claude-spare"), before.workspaces[1].subscriptions)
            val s = c.settingsUpdate(
                "1",
                listOf(
                    SettingsOp.SubscriptionAdd(SubscriptionSpec("claude-work", "claude-code", "~/.claude-work")),
                    SettingsOp.EnvUnset("app", "CLAUDE_CONFIG_DIR"),
                    SettingsOp.WorkspaceUpdate("app", subscriptions = listOf("claude-work")),
                    SettingsOp.AssistantUpdate(true, subscriptions = listOf("claude-spare")),
                ),
            )
            assertEquals(listOf("claude-work"), s.workspaces[0].subscriptions)
            assertEquals(listOf("claude-spare"), s.assistant?.subscriptions)
            assertEquals("bad-request", refusal { c.settingsUpdate(s.revision, listOf(SettingsOp.SubscriptionRemove("claude-work"))) }.code)
            assertEquals(
                "bad-request",
                refusal { c.settingsUpdate(s.revision, listOf(SettingsOp.WorkspaceUpdate("app", subscriptions = listOf("nope")))) }.code,
            )
            c.close()
        }
    }

    @Test
    fun aTeamLeadsOnlyBotsThatLeadNone() = runBlocking {
        withTimeout(10_000) {
            val c = open()
            val triage = SettingsOp.BotAdd("app", BotSpec("triage"))
            val lead = SettingsOp.BotUpdate("app", BotSpec("pr-watch", members = listOf("triage"), handoffsPerDay = 5))
            val s = c.settingsUpdate("1", listOf(triage, lead))
            val bot = s.workspaces.first { it.name == "app" }.bots.first { it.name == "pr-watch" }
            assertEquals(listOf("triage") to 5, bot.members to bot.handoffsPerDay)
            val nested = SettingsOp.BotUpdate("app", BotSpec("triage", members = listOf("pr-watch")))
            assertEquals("bad-request", refusal { c.settingsUpdate(s.revision, listOf(nested)) }.code)
            c.close()
        }
    }

    @Test
    fun claudeConnectorsTurnOnForAWorkspace() = runBlocking {
        withTimeout(10_000) {
            val c = open()
            assertEquals(false, c.settings().workspaces.first { it.name == "app" }.claudeConnectors)
            val s = c.settingsUpdate("1", listOf(SettingsOp.WorkspaceUpdate("app", claudeConnectors = true)))
            assertEquals(true, s.workspaces.first { it.name == "app" }.claudeConnectors)
            val kept = c.settingsUpdate(s.revision, listOf(SettingsOp.WorkspaceUpdate("app", parkAfter = Patch.Set("2d"))))
            assertEquals(true, kept.workspaces.first { it.name == "app" }.claudeConnectors)
            c.close()
        }
    }

    @Test
    fun aWorkspacesBotRulesAreReplacedTogether() = runBlocking {
        withTimeout(10_000) {
            val c = open()
            val before = c.settings().workspaces
            assertEquals(BotRules(listOf("WebSearch", "WebFetch"), listOf("Bash")), before.first { it.name == "app" }.botRules)
            assertEquals(BotRules(), before.first { it.name == "Personal" }.botRules)
            val rules = BotRules(listOf("WebSearch"), listOf("Bash", "mcp__claude_ai_Gmail"))
            val s = c.settingsUpdate("1", listOf(SettingsOp.WorkspaceUpdate("app", botRules = rules)))
            assertEquals(rules, s.workspaces.first { it.name == "app" }.botRules)
            val kept = c.settingsUpdate(s.revision, listOf(SettingsOp.WorkspaceUpdate("app", claudeConnectors = true)))
            assertEquals(rules, kept.workspaces.first { it.name == "app" }.botRules)
            c.close()
        }
    }

    @Test
    fun aBotsCommandsAreKeptAndChecked() = runBlocking {
        withTimeout(10_000) {
            val c = open()
            val bot = c.settings().workspaces.first { it.name == "app" }.bots.single()
            assertEquals(listOf("gh pr list:*"), bot.commands)
            val more = bot.spec.copy(commands = bot.commands + "/path/collect.sh delivery:*")
            val s = c.settingsUpdate("1", listOf(SettingsOp.BotUpdate("app", more)))
            assertEquals(more.commands, s.workspaces.first { it.name == "app" }.bots.single().commands)
            val wrapped = more.copy(commands = listOf("Bash(gh pr list:*)"))
            assertEquals("bad-request", refusal { c.settingsUpdate(s.revision, listOf(SettingsOp.BotUpdate("app", wrapped))) }.code)
            c.close()
        }
        // The talk's proposal replaces the whole Bot and keeps its commands.
        withTimeout(10_000) {
            val c = open()
            c.inbox(V2Request.InboxApply("d12"))
            assertEquals(listOf("gh pr list:*"), c.settings().workspaces.first { it.name == "app" }.bots.single().commands)
            c.close()
        }
    }

    @Test
    fun aRenamedBotKeepsItsRoutinesTeamAndItems() = runBlocking {
        withTimeout(10_000) {
            val c = open()
            c.watch()
            val lead = SettingsOp.BotAdd("app", BotSpec("lead", members = listOf("pr-watch")))
            val s0 = c.settingsUpdate("1", listOf(lead))
            val s = c.settingsUpdate(s0.revision, listOf(SettingsOp.BotRename("app", "pr-watch", "reviews")))
            val app = s.workspaces.first { it.name == "app" }
            assertEquals(listOf("reviews", "lead"), app.bots.map { it.name })
            assertEquals(listOf("reviews"), app.bots.first { it.name == "lead" }.members)
            assertEquals(listOf("gh pr list:*"), app.bots.first { it.name == "reviews" }.commands)
            assertEquals("reviews", app.routines.first { it.name == "scan" }.bot)
            val inbox = c.inbox()
            assertEquals(listOf("reviews", "reviews"), inbox.items.filter { it.id in setOf("d10", "d11") }.map { it.bot })
            assertEquals(listOf("reviews"), inbox.bots.map { it.bot })
            assertEquals("reviews", c.routines().first.first { it.name == "scan" }.bot)
            assertEquals("bad-request", refusal { c.settingsUpdate(s.revision, listOf(SettingsOp.BotRename("app", "reviews", "lead"))) }.code)
            assertEquals("bad-request", refusal { c.settingsUpdate(s.revision, listOf(SettingsOp.BotRename("app", "reviews", "Re Views"))) }.code)
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
