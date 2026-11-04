package tech.xdge.muxa.app

import android.content.Intent
import android.view.KeyEvent
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.semantics.getOrNull
import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.assertIsEnabled
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createEmptyComposeRule
import androidx.compose.ui.test.onAllNodesWithTag
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollToNode
import androidx.compose.ui.test.performTextInput
import androidx.compose.ui.test.performTouchInput
import androidx.compose.ui.test.swipeRight
import androidx.test.core.app.ActivityScenario
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import tech.xdge.muxa.protocol.InboxAction
import tech.xdge.muxa.protocol.ItemState
import tech.xdge.muxa.protocol.TaskStatus

/** Inbox mode (ADR 0033, 0035), against the demo's items at `work` level 3. */
@OptIn(ExperimentalTestApi::class)
@RunWith(AndroidJUnit4::class)
class ItemListTest {
    @get:Rule
    val compose = createEmptyComposeRule()
    private var scenario: ActivityScenario<MainActivity>? = null
    private val model get() = ApplicationProvider.getApplicationContext<MuxaApplication>().model

    @After
    fun close() {
        scenario?.close()
    }

    /** Opens the demo in Inbox mode: the sidebar is the list, on every layout (ADR 0035). */
    private fun launchOnInbox() {
        scenario = ActivityScenario.launch(demoIntent(SidebarMode.INBOX))
        waitForTag(row(ASK))
    }

    private fun paneShows(text: String) = SemanticsMatcher("pane shows \"$text\"") {
        it.config.getOrNull(SemanticsProperties.StateDescription)?.contains(text) == true
    }

    private fun exists(tag: String) = compose.onAllNodesWithTag(tag).fetchSemanticsNodes().isNotEmpty()

    private fun waitForTag(tag: String) = compose.waitUntilAtLeastOneExists(hasTestTag(tag), TIMEOUT)

    private fun click(tag: String) {
        waitForTag(tag)
        compose.onNodeWithTag(tag).performClick()
    }

    private fun row(id: String) = "item-row-$id"

    private fun waitForText(tag: String, text: String) =
        compose.waitUntilAtLeastOneExists(hasTestTag(tag) and hasText(text, substring = true), TIMEOUT)

    private val rowTag = SemanticsMatcher("an item row") {
        it.config.getOrNull(SemanticsProperties.TestTag)?.startsWith("item-row-") == true
    }

    /** The rows' ids top to bottom. */
    private fun rowIds(): List<String> = compose.onAllNodes(rowTag).fetchSemanticsNodes()
        .sortedBy { it.boundsInRoot.top }
        .map { it.config[SemanticsProperties.TestTag].removePrefix("item-row-") }

    private fun item(id: String) = model.state.inboxItems.first { it.id == id }

    /** The ask comes first; each row shows the question, then the Task and the age. */
    @Test
    fun theListShowsTheQuestionsInTheHostsOrder() {
        launchOnInbox()
        assertEquals(ASK, rowIds().first())
        waitForText(row(ASK), "Allow Bash: rm -rf build")
        waitForText(row(ASK), "Add sign-in with passkeys · ")
        waitForTag(row(REVIEW))
        assertEquals(listOf(ASK, FAILURE, REVIEW), model.inboxItems.take(3).map { it.id })
    }

    /** A review shows its summary and evidence; opening it marks it read; Accept removes it and marks the Task done. */
    @Test
    fun acceptingAReviewRemovesItAndMarksTheTaskDone() {
        launchOnInbox()
        click(row(REVIEW))
        waitForText("item-question", "Ready for review: Fix the login redirect")
        waitForText("item-summary", "redirect test passes")
        waitForText("item-changes", "+120 −40")
        waitForTag("item-doc-docs/design.md")
        waitForText("item-link", "PR #817")
        compose.waitUntil(TIMEOUT) { item(REVIEW).readAt != null }
        click("item-accept")
        compose.waitUntil(TIMEOUT) { model.inboxItems.none { it.id == REVIEW } }
        compose.waitUntilDoesNotExist(hasTestTag(row(REVIEW)), TIMEOUT)
        assertEquals(InboxAction.ACCEPT, item(REVIEW).answer)
        compose.waitUntil(TIMEOUT) { model.state.tasks?.record("t-login1")?.status == TaskStatus.DONE }
    }

    /** Redo sends the reply's text, and waits for one. */
    @Test
    fun redoNeedsATextAndSendsIt() {
        launchOnInbox()
        click(row(REVIEW))
        waitForTag("item-redo")
        compose.onNodeWithTag("item-redo").assertIsNotEnabled()
        compose.onNodeWithTag("item-reply").performTextInput("Keep the old redirect for one release.")
        compose.onNodeWithTag("item-redo").assertIsEnabled().performClick()
        compose.waitUntil(TIMEOUT) { item(REVIEW).state == ItemState.ANSWERED }
        assertEquals(InboxAction.REDO, item(REVIEW).answer)
        compose.waitUntilDoesNotExist(hasTestTag(row(REVIEW)), TIMEOUT)
    }

    /** An ask's Approve answers it; Reject is beside it. */
    @Test
    fun approvingAnAskAnswersIt() {
        launchOnInbox()
        click(row(ASK))
        waitForText("item-question", "Allow Bash: rm -rf build")
        waitForTag("item-reject")
        click("item-approve")
        compose.waitUntil(TIMEOUT) { item(ASK).answer == InboxAction.APPROVE }
        compose.waitUntilDoesNotExist(hasTestTag(row(ASK)), TIMEOUT)
    }

    /** A swipe asks for Later, and the item leaves the list until then. */
    @Test
    fun laterBySwipeHidesTheItem() {
        launchOnInbox()
        compose.onNodeWithTag(row(FAILURE)).performTouchInput { swipeRight() }
        click("later-in_an_hour")
        compose.waitUntil(TIMEOUT) { item(FAILURE).state == ItemState.LATER }
        assertNotNull(item(FAILURE).until)
        compose.waitUntil(TIMEOUT) { model.inboxItems.none { it.id == FAILURE } }
    }

    /** A proposal's Approve shows its action, target and content first. */
    @Test
    fun approvingAProposalShowsItFirst() {
        launchOnInbox()
        compose.onNodeWithTag("inbox-list").performScrollToNode(hasTestTag(row("e-000000a2")))
        click(row("e-000000a2"))
        click("item-approve")
        waitForText("approve-target", "linear:ENG-12")
        click("approve-confirm")
        compose.waitUntil(TIMEOUT) { model.state.inboxItems.first { it.id == "e-000000a2" }.state == ItemState.ANSWERED }
        compose.waitUntil(TIMEOUT) { model.state.tasks?.records?.count { it.title == "Close the stale issue ENG-12" } == 1 }
    }

    /** Ctrl+Shift+Down moves, Enter opens, Ctrl+Shift+Enter sends the main answer. */
    @Test
    fun keysMoveOpenAndAnswer() {
        launchOnInbox()
        compose.waitUntil(TIMEOUT) { model.itemFocus == ASK }
        key(KeyEvent.KEYCODE_DPAD_DOWN)
        compose.waitUntil(TIMEOUT) { model.itemFocus == FAILURE }
        if (model.selectedItem == null) key(KeyEvent.KEYCODE_ENTER, chord = false)
        compose.waitUntil(TIMEOUT) { model.selectedItem == FAILURE }
        waitForTag("item-retry")
        key(KeyEvent.KEYCODE_ENTER)
        compose.waitUntil(TIMEOUT) { item(FAILURE).answer == InboxAction.RETRY }
    }

    /**
     * At connect Inbox mode stays on its list, with no pane: a phone shows the list itself, a
     * tablet the first item beside it (ADR 0035).
     */
    @Test
    fun inboxModeStaysOnTheListAtConnect() {
        launchOnInbox()
        compose.waitUntil(TIMEOUT) { model.isConnected && !model.connecting }
        waitForTag("inbox-list")
        assertFalse(model.paneShown)
        compose.onNodeWithTag("composer").assertDoesNotExist()
        if (!model.phoneLayout) compose.waitUntil(TIMEOUT) { model.selectedItem == ASK }
        waitForTag("status-row")
    }

    /** Terminal shows the agent's terminal over the item; its row stays highlighted, and Back returns to it. */
    @Test
    fun terminalOpensTheAgentPaneAndReturnsToTheItem() {
        launchOnInbox()
        click(row(ASK))
        click("item-terminal")
        compose.waitUntilAtLeastOneExists(hasTestTag("pane") and paneShows("pane %1 100x30"), TIMEOUT)
        compose.onNodeWithTag("composer").assertDoesNotExist()
        assertEquals(AppModel.PaneView.AGENT, model.paneView)
        assertEquals(ASK, model.paneItem?.id)
        if (model.phoneLayout) {
            click("back")
            waitForText("item-question", "Allow Bash: rm -rf build")
            click("item-back")
            waitForTag("inbox-list")
        } else {
            click("show-list")
            waitForTag(row(ASK))
            assertEquals(ASK, model.selectedItem)
        }
    }

    /** Chat answers an `ask` in the agent's conversation. */
    @Test
    fun chatOpensTheAgentsChat() {
        launchOnInbox()
        click(row(ASK))
        click("item-chat")
        waitForTag("composer")
        assertEquals(AppModel.PaneView.CHAT, model.paneView)
    }

    /** Evidence opens the Reader of the item's pane; closing it returns to the item, not the pane. */
    @Test
    fun evidenceOpensTheReaderAndReturnsToTheItem() {
        launchOnInbox()
        click(row(REVIEW))
        click("item-doc-docs/design.md")
        waitForTag("reader")
        click(if (exists("reader-done")) "reader-done" else "reader-close")
        waitForText("item-question", "Ready for review: Fix the login redirect")
        assertFalse(model.paneShown)
    }

    /** Ctrl+Shift+O opens the focused item's terminal. */
    @Test
    fun ctrlShiftOOpensTheItemsTerminal() {
        launchOnInbox()
        compose.waitUntil(TIMEOUT) { model.itemFocus == ASK }
        key(KeyEvent.KEYCODE_O)
        compose.waitUntilAtLeastOneExists(hasTestTag("pane") and paneShows("pane %1 100x30"), TIMEOUT)
    }

    /** Ctrl+Shift+L, S and T pick Tasks, Sessions and Inbox; Ctrl+Shift+0 shows or hides the sidebar beside the pane. */
    @Test
    fun keysPickTheModes() {
        launchOnInbox()
        key(KeyEvent.KEYCODE_L)
        waitForTag("task-list")
        assertEquals(SidebarMode.TASKS, model.mode)
        key(KeyEvent.KEYCODE_S)
        waitForTag("session-list")
        key(KeyEvent.KEYCODE_T)
        waitForTag("inbox-list")
        assertEquals(SidebarMode.INBOX, model.mode)
        if (!model.phoneLayout) {
            click(row(ASK))
            click("item-terminal")
            compose.waitUntil(TIMEOUT) { !model.listShown }
            key(KeyEvent.KEYCODE_0)
            waitForTag("inbox-list")
            key(KeyEvent.KEYCODE_0)
            compose.waitUntilDoesNotExist(hasTestTag("inbox-list"), TIMEOUT)
        }
    }

    private fun key(code: Int, chord: Boolean = true) {
        val now = android.os.SystemClock.uptimeMillis()
        val meta = if (chord) KeyEvent.META_CTRL_ON or KeyEvent.META_SHIFT_ON else 0
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        instrumentation.sendKeySync(KeyEvent(now, now, KeyEvent.ACTION_DOWN, code, 0, meta))
        instrumentation.sendKeySync(KeyEvent(now, now, KeyEvent.ACTION_UP, code, 0, meta))
    }

    private companion object {
        const val TIMEOUT = 10_000L
        const val ASK = "i-0000a001"
        const val REVIEW = "i-0000a002"
        const val FAILURE = "i-0000a003"
    }
}
