package tech.xdge.muxa.session

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import tech.xdge.muxa.protocol.WorkItem
import tech.xdge.muxa.protocol.WorkState
import tech.xdge.muxa.protocol.WorkspaceInfo

class WorkItemsTest {
    private fun item(id: String, state: String, laterAt: Long? = null) =
        WorkItem(id, 1, "Title $id", WorkState(state), 1, 1, laterAt = laterAt)

    @Test
    fun theWorkSectionKeepsTheHostsOrderAndLeavesClosedOut() {
        val listed = listOf(
            item("w-1", "open"), item("w-2", "waiting-decision"), item("w-3", "open", laterAt = 5),
            item("w-4", "done"), item("w-5", "withdrawn"),
        ).listed()
        // Later moves an item down; it is never hidden (ADR 0040).
        assertEquals(listOf("w-1", "w-2", "w-3"), listed.map { it.id })
        assertTrue(listed.last().isLater)
    }

    @Test
    fun anUnknownStateReadsAsOpen() {
        assertEquals("open", item("w-1", "snoozed").stateLabel)
        assertEquals("waiting on decision", item("w-1", "waiting-decision").stateLabel)
        assertEquals("in progress", item("w-1", "in-progress").stateLabel)
    }

    @Test
    fun theFirstLineSkipsBlankLinesAndIsCut() {
        assertEquals("Look into it", firstLine("\n  \n  Look into it  \nmore"))
        assertEquals(200, firstLine("x".repeat(500)).length)
        assertEquals("", firstLine("   "))
    }

    @Test
    fun aCaptureDraftNeedsTextWithinTheLimit() {
        assertNotNull(CaptureDraft(text = "  ").problem)
        assertNotNull(CaptureDraft(text = "x".repeat(CAPTURE_MAX_BYTES + 1)).problem)
        assertNull(CaptureDraft(text = "Idea").problem)
        // One id per capture, kept across edits.
        val draft = CaptureDraft()
        assertEquals(draft.clientId, draft.copy(text = "Idea").clientId)
    }

    @Test
    fun startATaskInsteadTakesTheTextAsGoalAndItsFirstLineAsTitle() {
        val workspaces = listOf(WorkspaceInfo("work"), WorkspaceInfo("personal"))
        val task = CaptureDraft(text = "Fix the upload test\nIt fails on CI.", workspace = "personal").taskDraft(workspaces)
        assertEquals("personal", task.workspace)
        assertEquals("Fix the upload test", task.title)
        assertEquals("Fix the upload test\nIt fails on CI.", task.goal)
        assertNull(task.work)
        assertEquals("work", CaptureDraft(text = "Idea").taskDraft(workspaces).workspace)
    }

    @Test
    fun aTaskForAWorkItemKeepsItsIdAndRev() {
        val w = WorkItem("w-1", 4, "Plan the offsite", WorkState.OPEN, 1, 1, workspace = "gone")
        val task = TaskRecordDraft.forWork(w, listOf(WorkspaceInfo("work")))
        // A workspace the host no longer has falls back to the first.
        assertEquals("work", task.workspace)
        assertEquals("w-1", task.request?.work)
        assertEquals(4L, task.request?.workRev)
    }
}
