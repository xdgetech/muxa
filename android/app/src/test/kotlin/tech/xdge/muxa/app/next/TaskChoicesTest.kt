package tech.xdge.muxa.app.next

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import tech.xdge.muxa.v2.InboxItem
import tech.xdge.muxa.v2.ItemKind
import tech.xdge.muxa.v2.ItemLink
import tech.xdge.muxa.v2.ItemPlace
import tech.xdge.muxa.v2.ItemProposal
import tech.xdge.muxa.v2.ItemSuggestion
import tech.xdge.muxa.v2.LoopChecks
import tech.xdge.muxa.v2.LoopSpec
import tech.xdge.muxa.v2.PlaceSpec
import tech.xdge.muxa.v2.Workspace
import tech.xdge.muxa.v2.WorkspaceConnection
import tech.xdge.muxa.v2.WorkspaceLoop
import tech.xdge.muxa.v2.WorkspaceMcp

class TaskChoicesTest {
    private val app = Workspace("app", "/w", listOf("app", "docs"), loop = WorkspaceLoop(LoopChecks.List(listOf("make test", "make lint"))))

    @Test
    fun noRepoReadsEveryRepo() {
        val draft = TaskChoices(title = " Fix login ").inWorkspace(app).draft()
        assertEquals("app", draft.workspace)
        assertEquals("Fix login", draft.title)
        assertNull(draft.goal)
        assertEquals(listOf(PlaceSpec.AllRead), draft.places)
        assertNull(draft.loop)
    }

    @Test
    fun oneRepoIsABranchOrReadOnly() {
        val choices = TaskChoices(title = "t").inWorkspace(app).copy(repo = "app", branch = " fix-login ")
        assertEquals(listOf(PlaceSpec.Branch("app", "fix-login")), choices.draft().places)
        assertEquals(listOf(PlaceSpec.Read("app")), choices.copy(readOnly = true).draft().places)
        assertFalse("a branch needs a name", choices.copy(branch = " ").canStart)
        assertTrue(choices.copy(branch = " ", readOnly = true).canStart)
    }

    @Test
    fun aNewWorkspacePicksItsFirstCheckAndClearsTheLoop() {
        val choices = TaskChoices(title = "t", loop = true).inWorkspace(app)
        assertEquals("make test", choices.check)
        assertFalse(choices.loop)
        assertEquals(LoopSpec("make test", 5), choices.copy(loop = true, maxAttempts = 5).draft().loop)
    }

    @Test
    fun theToolsLineNamesEveryConnectionAndMcpServer() {
        val ws = app.copy(
            connections = listOf(WorkspaceConnection("github", true), WorkspaceConnection("linear", true)),
            mcp = listOf(WorkspaceMcp("docs", true)),
        )
        assertEquals("This Workspace's agents get: github, linear · MCP: docs", toolsLine(ws))
        assertEquals("This Workspace's agents get: MCP: docs", toolsLine(ws.copy(connections = emptyList())))
        assertNull(toolsLine(app))
    }

    @Test
    fun resumeStartsInTheItemsPlaceWithoutThePrefix() {
        val item = InboxItem("i1", ItemKind.TODO, "Resume: Tidy the notes", created = 0, source = "v1", place = ItemPlace("app", "/w/tasks/tidy"))
        val draft = TaskDraft.resume(item)
        assertEquals(TaskDraft("app", "Tidy the notes", null, listOf(PlaceSpec.Existing("/w/tasks/tidy")), plan = false, loop = null), draft)
        assertNull(TaskDraft.resume(item.copy(place = null)))
    }

    @Test
    fun aTitleIsNeeded() {
        assertFalse(TaskChoices(title = "  ").inWorkspace(app).canStart)
    }

    @Test
    fun startTaskOnAProposalTakesItAsTheGoal() {
        val item = InboxItem("i1", ItemKind.DECIDE, "Approve PR #57?", note = "CI passed.", workspace = "app", created = 0, source = "routine")
        assertEquals(TaskSeed("Approve PR #57?", "app", ""), TaskSeed.startTask(item))
        val proposed = item.copy(proposal = ItemProposal("review-approve", "example/app#57", "Approving."))
        assertEquals("review-approve on example/app#57:\n\nApproving.", TaskSeed.startTask(proposed).goal)
    }

    @Test
    fun aTalkIsAFolderTaskWithItsBot() {
        assertEquals(
            TaskDraft("app", "Talk to pr-watch", null, emptyList(), plan = false, loop = null, bot = "pr-watch"),
            TaskDraft.talk("app", "pr-watch"),
        )
    }

    @Test
    fun askingABotTakesTheItemAsTheGoal() {
        val item = InboxItem(
            "i", ItemKind.TODO, "Look at the flaky test", note = "Twice this week.", created = 1, source = "capture",
            links = listOf(ItemLink("https://ci/1", "CI run"), ItemLink("https://ci/2")),
            suggestion = ItemSuggestion(workspace = "app"),
        )
        val ask = TaskDraft.ask(item, "pr-watch")
        assertEquals("app" to "Ask pr-watch: Look at the flaky test", ask?.workspace to ask?.title)
        assertEquals("Look at the flaky test\n\nTwice this week.\n\nCI run: https://ci/1\nhttps://ci/2", ask?.goal)
        assertEquals("pr-watch", ask?.bot)
        assertNull(TaskDraft.ask(item.copy(suggestion = null), "pr-watch"))
        assertEquals("Personal", TaskDraft.ask(item.copy(suggestion = null, workspace = "Personal"), "b")?.workspace)
    }
}
