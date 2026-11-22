package tech.xdge.muxa.app.next

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Test
import tech.xdge.muxa.v2.ConnectionSpec
import tech.xdge.muxa.v2.McpSpec
import tech.xdge.muxa.v2.SecretState
import tech.xdge.muxa.v2.SettingsConnection
import tech.xdge.muxa.v2.SettingsOp
import tech.xdge.muxa.v2.SettingsRoutine
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
        val edited = RoutineDraft.of(r).copy(text = "New\n", cron = true, schedule = "0 9 * * *").op("app") as SettingsOp.RoutineUpdate
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
}
