package tech.xdge.muxa.app.next

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import tech.xdge.muxa.v2.BotCount
import tech.xdge.muxa.v2.BotSpec
import tech.xdge.muxa.v2.ConnectionSpec
import tech.xdge.muxa.v2.McpSpec
import tech.xdge.muxa.v2.RoutineSpec
import tech.xdge.muxa.v2.SecretState
import tech.xdge.muxa.v2.SettingsAssistant
import tech.xdge.muxa.v2.SettingsBot
import tech.xdge.muxa.v2.SettingsConnection
import tech.xdge.muxa.v2.SettingsOp
import tech.xdge.muxa.v2.SettingsRoutine
import tech.xdge.muxa.v2.SettingsWorkspace
import tech.xdge.muxa.v2.WorkerLoop
import tech.xdge.muxa.v2.WorkerSpec

class SettingsDraftsTest {
    @Test
    fun aBlankSecretKeepsTheOneInTheKeychain() {
        val draft = ConnectionDraft.of(SettingsConnection("github", "GITHUB_TOKEN", "me", "read", SecretState.SET))
        assertEquals(SettingsOp.ConnectionUpdate("app", ConnectionSpec("github", "GITHUB_TOKEN", "me", "read")), draft.op("app"))
        val added = ConnectionDraft(true, " linear ", "LINEAR_API_KEY", secret = "s3cret").op("app") as SettingsOp.ConnectionAdd
        assertEquals("s3cret", added.secret)
        assertEquals(ConnectionSpec("linear", "LINEAR_API_KEY"), added.connection)
    }

    @Test
    fun aDraftNeverPrintsItsSecret() {
        assertFalse("s3cret" in ConnectionDraft(true, "a", "B", secret = "s3cret").toString())
    }

    @Test
    fun anMcpServerReadsItsLinesAndPairs() {
        val cmd = McpDraft(true, "docs", command = "npx", args = "-y\n docs-mcp \n", env = "A=1\nB = two=2", connections = setOf("github"))
        assertEquals(McpSpec("docs", command = "npx", args = listOf("-y", "docs-mcp"), env = mapOf("A" to "1", "B" to "two=2"), connections = listOf("github")), cmd.spec())
        val url = McpDraft(true, "linear", remote = true, url = "https://x/mcp", type = McpDraft.SSE, headers = "X-Team: core")
        assertEquals(McpSpec("linear", url = "https://x/mcp", type = "sse", headers = mapOf("X-Team" to "core")), url.spec())
        assertEquals("Write each variable as KEY=value.", cmd.copy(env = "nokey").problem)
        assertEquals(cmd.spec(), McpDraft.of(cmd.spec()).spec())
    }

    @Test
    fun aRoutineSendsItsTextOnlyWhenItChanged() {
        val r = SettingsRoutine("digest", at = "09:00", instructions = "routines/digest.md", project = "app", instructionsText = "Old\n")
        val same = RoutineDraft.of(r).op("app") as SettingsOp.RoutineUpdate
        assertNull(same.instructionsText)
        assertEquals("app", same.routine.project)
        val edited = RoutineDraft.of(r).copy(text = "New\n", timing = RoutineTiming.CRON, schedule = "0 9 * * *").op("app") as SettingsOp.RoutineUpdate
        assertEquals("New\n", edited.instructionsText)
        assertNull(edited.routine.at)
        assertEquals("0 9 * * *", edited.routine.schedule)
    }

    @Test
    fun anUnreadableInstructionFileIsNotEmptied() {
        val r = SettingsRoutine("digest", instructions = "routines/digest.md", instructionsText = null)
        assertNull((RoutineDraft.of(r).op("app") as SettingsOp.RoutineUpdate).instructionsText)
    }

    @Test
    fun aNewRoutineGetsAFileNamedAfterIt() {
        val add = RoutineDraft(true, "weekly", text = "Do it.").op("app") as SettingsOp.RoutineAdd
        assertEquals("routines/weekly.md", add.routine.instructions)
        assertEquals("Do it.", add.instructionsText)
    }

    @Test
    fun aWorkerKeepsItsLoopLimitsAndNoWorkspacesMeansAll() {
        val w = WorkerSpec("reviewer", agent = "codex", loop = WorkerLoop("make test", maxAttempts = 4))
        val spec = WorkerDraft.of(w).copy(check = "make lint").spec()
        assertEquals(WorkerLoop("make lint", maxAttempts = 4), spec.loop)
        assertNull(spec.workspaces)
        assertNull(WorkerDraft.of(w).copy(check = " ").spec().loop)
    }

    @Test
    fun aRoutineKeepsItsBotAndReadOnly() {
        val r = SettingsRoutine("scan", schedule = "*/30 9-18 * * 1-5", instructions = "routines/scan.md", bot = "pr-watch", readOnly = true)
        val spec = RoutineDraft.of(r).spec()
        assertEquals("pr-watch", spec.bot)
        assertEquals(true, spec.readOnly)
        assertNull(RoutineDraft.of(r).copy(bot = null).spec().bot)
    }

    @Test
    fun aBotNameIsLowercase() {
        val d = BotDraft(true, "pr-watch", soul = "You watch PRs.")
        assertNull(d.problem)
        assertEquals("A name has only lowercase letters, digits, - and _.", d.copy(name = "PR Watch").problem)
        assertEquals("The Soul is at most 12 KiB.", d.copy(soul = "x".repeat(12 * 1024 + 1)).problem)
        val add = d.copy(description = " Watches PRs ").op("app") as SettingsOp.BotAdd
        assertEquals(BotSpec("pr-watch", "Watches PRs", null, true), add.bot)
        assertEquals("You watch PRs.", add.soulText)
    }

    @Test
    fun aBotSendsItsSoulOnlyWhenItChanged() {
        val b = SettingsBot("pr-watch", soulText = "Old\n", memoryText = "- learned\n", memoryVersions = 2)
        val same = BotDraft.of(b).op("app") as SettingsOp.BotUpdate
        assertNull(same.soulText)
        assertEquals(BotSpec("pr-watch"), same.bot)
        assertEquals("New", (BotDraft.of(b).copy(soul = "New").op("app") as SettingsOp.BotUpdate).soulText)
        assertNull((BotDraft.of(b.copy(soulText = null)).op("app") as SettingsOp.BotUpdate).soulText)
    }

    @Test
    fun aBotsSummaryCountsItsRoutinesAndHowItsItemsEnded() {
        val bot = SettingsBot("pr-watch", worker = "reviewer")
        val ws = SettingsWorkspace(
            "app", "/w",
            routines = listOf(SettingsRoutine("scan", instructions = "s.md", bot = "pr-watch"), SettingsRoutine("other", instructions = "o.md")),
        )
        // ADR 0087: a Bot's worker is ignored by the host, so the summary leaves it out.
        assertEquals("1 Routine", botSummary(bot, ws, emptyList()))
        val counts = listOf(BotCount("Personal", "pr-watch", 9, 9), BotCount("app", "pr-watch", 4, 1))
        assertEquals("1 Routine · 4 done · 1 deleted", botSummary(bot, ws, counts))
    }

    @Test
    fun aRoutineOnReviewHasNoTime() {
        val r = SettingsRoutine("check", at = "09:00", timezone = "Europe/Berlin", instructions = "routines/check.md", instructionsText = "")
        val spec = RoutineDraft.of(r).copy(timing = RoutineTiming.REVIEW).spec()
        assertEquals(listOf("review", null, null, null), listOf(spec.on, spec.at, spec.schedule, spec.timezone))
        assertEquals(RoutineTiming.REVIEW, RoutineDraft.of(SettingsRoutine.of(spec, "")).timing)
        assertEquals("routine.timing.review", "routine.timing.${RoutineTiming.REVIEW}")
    }

    @Test
    fun aTeamLeadPicksOtherBotsThatLeadNone() {
        val bots = listOf(
            SettingsBot("lead", members = listOf("a")),
            SettingsBot("a"),
            SettingsBot("b"),
            SettingsBot("other-lead", members = listOf("b")),
        )
        assertEquals(listOf("a", "b"), BotDraft.memberChoices("lead", bots))
        assertEquals(emptyList<String>(), BotDraft.memberChoices("a", bots))
        assertEquals(listOf("a", "b"), BotDraft.memberChoices("new", bots))
        val d = BotDraft.of(bots[0])
        assertEquals(BotSpec("lead", members = listOf("a"), handoffsPerDay = 5), d.copy(handoffs = "5").spec())
        assertEquals("Handoffs a day is a number from 1 to 50.", d.copy(handoffs = "51").problem)
        assertEquals(BotSpec("lead"), d.copy(members = emptySet(), handoffs = "5").spec())
        assertEquals("1 Routine · Leads a", botSummary(bots[0], SettingsWorkspace("w", "/w", routines = listOf(SettingsRoutine("r", instructions = "r.md", bot = "lead"))), emptyList()))
    }

    @Test
    fun theAssistantSendsItsSoulOnlyWhenItChanged() {
        val a = SettingsAssistant(true, "reviewer", "Old\n", "- memory\n")
        assertEquals(SettingsOp.AssistantUpdate(false, "reviewer", null), AssistantDraft.of(a).copy(enabled = false).op())
        assertEquals(SettingsOp.AssistantUpdate(true, null, "New"), AssistantDraft.of(a).copy(worker = null, soul = "New").op())
    }

    @Test
    fun aProposedOpReadsAsALine() {
        val off = RoutineSpec("nightly-sweep", instructions = "r.md", enabled = false)
        assertEquals("Add Routine nightly-sweep (off)", proposalLine(SettingsOp.RoutineAdd("app", off)))
        assertEquals("Change Bot pr-watch", proposalLine(SettingsOp.BotUpdate("app", BotSpec("pr-watch"))))
    }

    @Test
    fun aBotsCommandIsOneLineWithoutParentheses() {
        assertNull(BotDraft.commandProblem("/path/collect.sh delivery:*"))
        assertEquals("Write a command.", BotDraft.commandProblem("  "))
        assertEquals("A command is at most 500 bytes.", BotDraft.commandProblem("x".repeat(501)))
        assertEquals("Write the command without Bash( and ), and without parentheses.", BotDraft.commandProblem("Bash(ls)"))
        val b = SettingsBot("collector", commands = listOf("gh pr list:*"))
        assertEquals(listOf("gh pr list:*"), BotDraft.of(b).spec().commands)
        assertEquals(BotDraft.commandProblem("a(b"), BotDraft.of(b).copy(commands = listOf("a(b")).problem)
    }

    @Test
    fun aBotIsRenamedOnlyToAFreeValidName() {
        assertEquals("pr-reviews", BotDraft.typedName("PR Reviews"))
        val names = listOf("pr-watch", "triage")
        assertTrue(BotDraft.canRename("pr-watch", "reviews", names))
        assertFalse(BotDraft.canRename("pr-watch", "pr-watch", names))
        assertFalse(BotDraft.canRename("pr-watch", "triage", names))
        assertFalse(BotDraft.canRename("pr-watch", "", names))
        assertFalse(BotDraft.canRename("pr-watch", "a".repeat(65), names))
        assertFalse(BotDraft.canRename("pr-watch", "pr.watch", names))
    }
}
