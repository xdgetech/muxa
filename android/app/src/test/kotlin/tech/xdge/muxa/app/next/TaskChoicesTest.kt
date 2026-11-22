package tech.xdge.muxa.app.next

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Test
import tech.xdge.muxa.v2.LoopChecks
import tech.xdge.muxa.v2.LoopSpec
import tech.xdge.muxa.v2.PlaceSpec
import tech.xdge.muxa.v2.Workspace
import tech.xdge.muxa.v2.WorkspaceLoop

class TaskChoicesTest {
    private val app = Workspace("app", "/w", listOf("app", "docs"), loop = WorkspaceLoop(LoopChecks.List(listOf("make test", "make lint"))))

    @Test
    fun aFolderTaskHasOneFolderAndItsReadOnlyRepos() {
        val draft = TaskChoices(title = " Fix login ").inWorkspace(app).copy(readOnly = setOf("docs")).draft()
        assertEquals("app", draft.workspace)
        assertEquals("Fix login", draft.title)
        assertNull(draft.goal)
        assertEquals(listOf(PlaceSpec.Folder, PlaceSpec.Read("docs")), draft.places)
        assertNull(draft.loop)
    }

    @Test
    fun worktreesSkipUnnamedRowsAndARepoIsNotAlsoReadOnly() {
        val choices = TaskChoices(title = "t").inWorkspace(app).copy(
            worktrees = true,
            branches = listOf(BranchChoice("app", "fix-login"), BranchChoice("docs", " ")),
            readOnly = setOf("app", "docs"),
        )
        assertEquals(listOf(PlaceSpec.Branch("app", "fix-login"), PlaceSpec.Read("docs")), choices.draft().places)
    }

    @Test
    fun aNewWorkspacePicksItsFirstCheckAndClearsTheTools() {
        val choices = TaskChoices(title = "t", connections = setOf("linear"), loop = true).inWorkspace(app)
        assertEquals("make test", choices.check)
        assertEquals(emptySet<String>(), choices.connections)
        assertFalse(choices.loop)
        assertEquals(LoopSpec("make test", 5), choices.copy(loop = true, maxAttempts = 5).draft().loop)
    }

    @Test
    fun aTitleIsNeeded() {
        assertFalse(TaskChoices(title = "  ").inWorkspace(app).canStart)
    }
}
