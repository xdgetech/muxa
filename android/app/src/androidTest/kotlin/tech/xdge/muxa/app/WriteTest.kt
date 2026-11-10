package tech.xdge.muxa.app

import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.junit4.createEmptyComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.test.performScrollToNode
import androidx.compose.ui.test.performTextInput
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
 * Write proposals (ADR 0052 §7) against the demo at `work` level 13: the `write` item with its whole
 * content as plain text, Approve that confirms the target and starts one Task of the writer, Reject with
 * a reason, a Result's writes in Work detail, and the writer where staff show. Level 12 shows none.
 */
@OptIn(ExperimentalTestApi::class)
@RunWith(AndroidJUnit4::class)
class WriteTest {
    @get:Rule
    val compose = createEmptyComposeRule()
    private var scenario: ActivityScenario<MainActivity>? = null
    private val model get() = ApplicationProvider.getApplicationContext<MuxaApplication>().model

    @After
    fun close() {
        scenario?.close()
    }

    private fun launchOnInbox() {
        scenario = ActivityScenario.launch(writesDemoIntent())
        waitForTag("today-row-$OPEN_WORK")
    }

    private fun waitForTag(tag: String) = compose.waitUntilAtLeastOneExists(hasTestTag(tag), TIMEOUT)

    private fun waitForText(tag: String, text: String) = compose.waitForTextIn(tag, text, TIMEOUT)

    private fun scrollTo(tag: String) {
        compose.onNodeWithTag("inbox-list").performScrollToNode(hasTestTag(tag))
        waitForTag(tag)
    }

    private fun detailScrollTo(tag: String) {
        compose.onNodeWithTag("work-detail").performScrollToNode(hasTestTag(tag))
        waitForTag(tag)
    }

    private fun openItem(id: String) {
        scrollTo("item-row-$id")
        compose.click("item-row-$id")
        waitForTag("item-write")
    }

    private fun openWork(id: String) {
        scrollTo("work-row-$id")
        compose.click("work-row-$id")
        compose.waitUntil(TIMEOUT) { model.workDetail == id }
    }

    /** The text the content block shows, exactly. */
    private fun shownContent(): String {
        compose.onNodeWithTag("item-write-content").performScrollTo()
        return compose.onNodeWithTag("item-write-content").fetchSemanticsNode().config[SemanticsProperties.Text].joinToString("") { it.text }
    }

    private fun writerTasks() = model.state.tasks?.records.orEmpty().filter { it.worker == WRITER }

    private fun item(id: String) = model.state.inboxItems.first { it.id == id }

    /** The content shows as typed: Markdown stays as its characters and a URL is plain text. */
    @Test
    fun theItemShowsTheWholeContentAsPlainText() {
        launchOnInbox()
        openItem(POST)
        waitForText("item-question", "work-writer proposes to write to slack:#release")
        waitForText("item-detail", "The release is tagged")
        val shown = shownContent()
        assertEquals(item(POST).write?.content, shown)
        assertTrue(shown.contains("**Faster sync**"))
        assertTrue(shown.contains("https://example.com/releases/2.4"))
        assertFalse(compose.exists("item-write-older"))
    }

    /** An 8 KiB content shows every line, and a write from an older Result says so. */
    @Test
    fun anOlderWriteShowsEveryLineOfItsContent() {
        launchOnInbox()
        openItem(COMMENT)
        val shown = shownContent()
        assertEquals(item(COMMENT).write?.content, shown)
        val lines = shown.lines()
        assertEquals(100, lines.size)
        assertTrue(lines.first().startsWith("Line 001:") && lines.last().startsWith("Line 100:"))
        assertTrue(shown.toByteArray().size > 8_000)
        compose.onNodeWithTag("item-write-older").performScrollTo()
        waitForText("item-write-older", "From an older result (v1)")
    }

    /** Approve confirms the target once more; Cancel sends nothing, Approve starts one Task of the writer. */
    @Test
    fun approveConfirmsTheTargetAndStartsOneTaskOfTheWriter() {
        launchOnInbox()
        openItem(POST)
        compose.click("item-${InboxAction.APPROVE.wire}")
        waitForText("approve-write-target", "slack:#release")
        compose.click("approve-write-cancel")
        compose.waitUntilDoesNotExist(hasTestTag("approve-write-target"), TIMEOUT)
        assertEquals(ItemState.OPEN, item(POST).state)
        compose.click("item-${InboxAction.APPROVE.wire}")
        compose.click("approve-write-confirm")
        // The demo refuses an approve without the shown write's hash: a Task proves the hash was sent.
        compose.waitUntil(TIMEOUT) { writerTasks().size == 1 }
        val task = writerTasks().single()
        assertEquals("Post to #release", task.title)
        assertEquals(WORK, task.work)
        assertEquals(task.id, item(POST).task)
    }

    /** Reject takes an optional reason and starts nothing. */
    @Test
    fun rejectTakesAReasonAndStartsNothing() {
        launchOnInbox()
        openItem(COMMENT)
        compose.click("item-${InboxAction.REJECT.wire}")
        waitForText("reject-reason", "Nothing is written")
        compose.onNodeWithTag("reject-reason").performTextInput("Not in this ticket")
        compose.click("reject-confirm")
        compose.waitUntil(TIMEOUT) { item(COMMENT).answer == InboxAction.REJECT }
        assertTrue(writerTasks().isEmpty())
    }

    /** Work detail lists each Result's writes with their state; Open Item leads to Approve; an approved write names its Task. */
    @Test
    fun workDetailListsEachResultsWrites() {
        launchOnInbox()
        openWork(WORK)
        detailScrollTo("write-$REQUEST-2-1-0")
        waitForText("write-$REQUEST-2-1-0", "Post to #release")
        waitForText("write-$REQUEST-2-1-0-state", "Waits for your approval")
        detailScrollTo("result-writes-older-$REQUEST-1-1")
        waitForText("write-$REQUEST-1-1-0", "linear:ENG-12")
        detailScrollTo("write-$REQUEST-2-1-0-open")
        compose.click("write-$REQUEST-2-1-0-open")
        waitForTag("item-write")
        compose.click("item-${InboxAction.APPROVE.wire}")
        compose.click("approve-write-confirm")
        compose.waitUntil(TIMEOUT) { writerTasks().size == 1 }
        compose.runOnIdle { model.workDetail = WORK }
        detailScrollTo("write-$REQUEST-2-1-0-task")
        waitForText("write-$REQUEST-2-1-0-task", "Task started")
    }

    /** The writer shows under the staff picker's place, marked "writes", with its description and the mode. */
    @Test
    fun theWriterShowsWhereStaffShow() {
        launchOnInbox()
        openWork(WORK)
        detailScrollTo("ask-writer")
        waitForText("ask-writer", "$WRITER · writes")
        waitForText("ask-writer", "Posts approved messages")
        waitForText("ask-writer-mode", "Each write waits for your approval")
    }

    /** A level 12 host keeps today's UI: no `write` item, no writes in Work detail, no writer. */
    @Test
    fun aLevel12HostShowsNone() {
        scenario = ActivityScenario.launch(staffDemoIntent())
        waitForTag("today-row-$OPEN_WORK")
        assertFalse(model.state.inboxItems.any { it.kind == ItemKind.WRITE })
        openWork(OPEN_WORK)
        detailScrollTo("work-ask")
        assertFalse(compose.exists("ask-writer"))
    }

    private companion object {
        const val TIMEOUT = 15_000L
        const val OPEN_WORK = "w-0000b001"
        const val WORK = "w-0000b006"
        const val REQUEST = "q-0000a003"
        const val POST = "i-0000d101"
        const val COMMENT = "i-0000d102"
        const val WRITER = "work-sender"
    }
}
