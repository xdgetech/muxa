package tech.xdge.muxa.app

import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.junit4.createEmptyComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.test.performScrollToNode
import androidx.test.core.app.ActivityScenario
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import tech.xdge.muxa.app.ui.wire
import tech.xdge.muxa.protocol.InboxAction
import tech.xdge.muxa.protocol.ItemKind
import tech.xdge.muxa.protocol.ItemState

/**
 * Workspace memory (ADR 0053 §7) against the demo at `work` level 14: the `memory` item with its
 * whole text as plain text, Approve that names the workspace once more, the Task detail line after
 * the end, and each workspace's mode. Level 13 shows none.
 */
@OptIn(ExperimentalTestApi::class)
@RunWith(AndroidJUnit4::class)
class MemoryTest {
    @get:Rule
    val compose = createEmptyComposeRule()
    private var scenario: ActivityScenario<MainActivity>? = null
    private val model get() = ApplicationProvider.getApplicationContext<MuxaApplication>().model

    @After
    fun close() {
        scenario?.close()
    }

    private fun waitForTag(tag: String) = compose.waitUntilAtLeastOneExists(hasTestTag(tag), TIMEOUT)

    private fun waitForText(tag: String, text: String) = compose.waitForTextIn(tag, text, TIMEOUT)

    private fun item() = model.state.inboxItems.first { it.id == ITEM }

    private fun openItem() {
        scenario = ActivityScenario.launch(memoryDemoIntent())
        waitForTag("inbox-list")
        compose.waitUntil(TIMEOUT) { model.state.inboxItems.any { it.id == ITEM } }
        compose.onNodeWithTag("inbox-list").performScrollToNode(hasTestTag("item-row-$ITEM"))
        compose.click("item-row-$ITEM")
        waitForTag("item-memory")
    }

    private fun openTask(mode: SidebarMode = SidebarMode.TASKS) {
        if (scenario == null) scenario = ActivityScenario.launch(memoryDemoIntent(mode))
        waitForTag("task-list")
        compose.waitUntil(TIMEOUT) { model.state.tasks?.records.orEmpty().any { it.id == TASK } }
        compose.onNodeWithTag("task-list").performScrollToNode(hasTestTag("task-$TASK"))
        compose.onNodeWithTag("task-$TASK").performClick()
        waitForTag("task-detail")
    }

    /** The text the block shows, exactly. */
    private fun shownText(): String {
        compose.onNodeWithTag("item-memory-text").performScrollTo()
        return compose.onNodeWithTag("item-memory-text").fetchSemanticsNode().config[SemanticsProperties.Text].joinToString("") { it.text }
    }

    /** A 4 KiB text shows whole, every line, before Approve. */
    @Test
    fun theItemShowsEveryLineOfTheText() {
        openItem()
        waitForText("item-question", "Cache the search results proposes to add to the workspace memory")
        val shown = shownText()
        assertEquals(item().memory?.text, shown)
        val lines = shown.lines()
        assertEquals(50, lines.size)
        assertTrue(lines.first().startsWith("Line 01:") && lines.last().startsWith("Line 50:"))
        assertTrue(shown.toByteArray().size in 4_000..4_096)
        waitForTag("item-${InboxAction.APPROVE.wire}")
        waitForTag("item-${InboxAction.REJECT.wire}")
    }

    /** Approve names the workspace once more; Cancel sends nothing, Add answers the item with the shown text's hash. */
    @Test
    fun approveNamesTheWorkspaceAndAddsTheText() {
        openItem()
        compose.click("item-${InboxAction.APPROVE.wire}")
        waitForText("approve-memory-workspace", "work")
        compose.click("approve-memory-cancel")
        compose.waitUntilDoesNotExist(hasTestTag("approve-memory-workspace"), TIMEOUT)
        assertEquals(ItemState.OPEN, item().state)
        compose.click("item-${InboxAction.APPROVE.wire}")
        compose.click("approve-memory-confirm")
        // The demo refuses an approve without the shown text's hash: an answered item proves it was sent.
        compose.waitUntil(TIMEOUT) { item().answer == InboxAction.APPROVE }
        assertEquals(ItemState.ANSWERED, item().state)
    }

    /** The done Task's detail names the proposal and its state; Open Item leads to Approve; then it reads "Added". */
    @Test
    fun theTaskDetailSaysWhatTheEndProposed() {
        openTask()
        compose.onNodeWithTag("task-memory").performScrollTo()
        waitForText("task-memory", "Proposed to the workspace memory · Waits for your approval in the Inbox")
        compose.click("task-memory-open")
        waitForTag("item-memory")
        compose.click("item-${InboxAction.APPROVE.wire}")
        compose.click("approve-memory-confirm")
        compose.waitUntil(TIMEOUT) { item().answer == InboxAction.APPROVE }
        compose.runOnIdle { model.selectMode(SidebarMode.TASKS) }
        openTask()
        waitForText("task-memory", "Added to the workspace memory")
        assertFalse(compose.exists("task-memory-open"))
    }

    /** The Task's workspace shows its memory mode and when WORKSPACE.md last changed. */
    @Test
    fun theWorkspaceShowsItsMemoryMode() {
        openTask()
        compose.onNodeWithTag("task-workspace-memory").performScrollTo()
        waitForText("task-workspace-memory-mode", "asks you in the Inbox")
        waitForText("task-workspace-memory-at", "WORKSPACE.md changed")
    }

    /** A level 13 host keeps today's UI: no `memory` item, no mode. */
    @Test
    fun aLevel13HostShowsNone() {
        scenario = ActivityScenario.launch(writesDemoIntent())
        waitForTag("inbox-list")
        compose.waitUntil(TIMEOUT) { model.state.inboxItems.isNotEmpty() }
        assertFalse(model.state.inboxItems.any { it.kind == ItemKind.MEMORY })
        assertTrue(model.state.tasks?.workspaces.orEmpty().all { it.memory == null })
    }

    private companion object {
        const val TIMEOUT = 15_000L
        const val ITEM = "i-0000e101"
        const val TASK = "t-memo01"
    }
}
