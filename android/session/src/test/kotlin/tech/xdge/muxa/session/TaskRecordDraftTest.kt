package tech.xdge.muxa.session

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class TaskRecordDraftTest {
    private val ready = TaskRecordDraft(workspace = "work", title = "Title", goal = "Goal")

    @Test
    fun aTaskNeedsAWorkspaceATitleAndAGoal() {
        assertEquals("Pick a workspace.", ready.copy(workspace = "").problem)
        assertEquals("Give the task a title.", ready.copy(title = " ").problem)
        assertEquals("Describe the goal.", ready.copy(goal = "").problem)
        assertNull(ready.problem)
    }

    @Test
    fun withoutARepoNothingAboutGitIsSent() {
        val req = ready.copy(project = " ").request!!
        assertNull(req.project)
        assertNull(req.repo)
        assertNull(req.branch)
        assertEquals("A branch needs a repository.", ready.copy(branch = "x").problem)
    }

    @Test
    fun aRepoNeedsAnAbsolutePathAndAGoodBranch() {
        val repo = ready.copy(repoDir = "/src/app")
        assertEquals("Name the branch.", repo.problem)
        assertEquals("The repository path must start with /.", repo.copy(repoDir = "src", branch = "b").problem)
        assertEquals("A branch name cannot start with -.", repo.copy(branch = "-x").problem)
        val req = repo.copy(branch = " feat/x ", base = "").request!!
        assertEquals("feat/x", req.branch)
        assertNull(req.base)
    }
}
