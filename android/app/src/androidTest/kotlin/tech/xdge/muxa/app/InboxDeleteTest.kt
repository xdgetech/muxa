package tech.xdge.muxa.app

import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createEmptyComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performScrollToNode
import androidx.test.core.app.ActivityScenario
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.After
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/** Protect and Delete Old (ADR 0046), against the demo at `work` level 8. */
@OptIn(ExperimentalTestApi::class)
@RunWith(AndroidJUnit4::class)
class InboxDeleteTest {
    @get:Rule
    val compose = createEmptyComposeRule()
    private var scenario: ActivityScenario<MainActivity>? = null
    private val model get() = ApplicationProvider.getApplicationContext<MuxaApplication>().model

    @After
    fun close() {
        scenario?.close()
    }

    /** [oldItems]: the failure and the last proposal are four days old. */
    private fun launchOnInbox(oldItems: Boolean = false) {
        scenario = ActivityScenario.launch(demoIntent(SidebarMode.INBOX).putExtra(MainActivity.EXTRA_OLD_ITEMS, oldItems))
        compose.waitForTag(row(REVIEW))
    }

    private fun row(id: String) = "item-row-$id"

    /** The row merges the lock's TalkBack name into its own. */
    private fun lock(id: String) = hasTestTag(row(id)) and hasContentDescription("Protected")

    private fun item(id: String) = model.state.inboxItems.first { it.id == id }

    private fun scrollTo(id: String) = compose.onNodeWithTag("inbox-list").performScrollToNode(hasTestTag(row(id)))

    /** On a phone the item covers the list; beside it, the list stays. */
    private fun backToList() {
        if (compose.exists("item-back")) compose.click("item-back")
        compose.waitForTag("inbox-list")
    }

    private fun toggleProtect(id: String, tag: String) {
        scrollTo(id)
        compose.click(row(id))
        compose.click("item-more")
        compose.click(tag)
    }

    private fun deleteOld(days: Int) {
        compose.click("inbox-more")
        compose.click("inbox-delete-old")
        compose.click("inbox-delete-old-$days")
    }

    private fun waitForMessage(text: String) =
        compose.waitUntilAtLeastOneExists(hasTestTag("delete-old-message") and hasText(text), TIMEOUT)

    /** Protect in the item's … menu shows the lock after the age; Unprotect clears it. */
    @Test
    fun protectShowsTheLock() {
        launchOnInbox()
        toggleProtect(REVIEW, "item-more-protect")
        compose.waitUntil(TIMEOUT) { item(REVIEW).protectedAt != null }
        compose.click("item-more")
        compose.waitUntilAtLeastOneExists(hasTestTag("item-more-unprotect") and hasText("Unprotect"), TIMEOUT)
        compose.click("item-more-unprotect")
        compose.waitUntil(TIMEOUT) { item(REVIEW).protectedAt == null }
        compose.click("item-more")
        compose.click("item-more-protect")
        compose.waitUntil(TIMEOUT) { item(REVIEW).protectedAt != null }
        backToList()
        compose.waitUntilAtLeastOneExists(lock(REVIEW), TIMEOUT)
        compose.onNode(lock(ASK)).assertDoesNotExist()
    }

    /** Delete Old names the count, removes the old unprotected items and keeps a protected one however old. */
    @Test
    fun deleteOldRemovesOldItemsButKeepsProtectedOnes() {
        launchOnInbox(oldItems = true)
        toggleProtect(OLD_PROPOSAL, "item-more-protect")
        compose.waitUntil(TIMEOUT) { item(OLD_PROPOSAL).protectedAt != null }
        backToList()
        deleteOld(3)
        waitForMessage("Delete 1 item older than 3 days? Protected items stay.")
        compose.click("delete-old-confirm")
        compose.waitUntil(TIMEOUT) { model.state.inboxItems.none { it.id == OLD_FAILURE } }
        compose.waitUntilDoesNotExist(hasTestTag(row(OLD_FAILURE)), TIMEOUT)
        assertNotNull(item(OLD_PROPOSAL).protectedAt)
        scrollTo(OLD_PROPOSAL)
        compose.waitUntilAtLeastOneExists(lock(OLD_PROPOSAL), TIMEOUT)
        // Nothing unprotected is left that old: only OK.
        deleteOld(5)
        waitForMessage("No unprotected items are older than 5 days.")
        compose.onNodeWithTag("delete-old-confirm").assertDoesNotExist()
        compose.click("delete-old-ok")
        assertNull(model.inboxPrompt)
    }

    /** A host below `work:8` shows no Delete Old and no Protect. */
    @Test
    fun anOlderHostOffersNeither() {
        scenario = ActivityScenario.launch(demoIntent(SidebarMode.INBOX).putExtra(MainActivity.EXTRA_WORK3, true))
        compose.waitForTag(row(REVIEW))
        compose.onNodeWithTag("inbox-more").assertDoesNotExist()
        compose.click(row(REVIEW))
        compose.click("item-more")
        compose.waitForTag("item-more-accept")
        compose.onNodeWithTag("item-more-protect").assertDoesNotExist()
    }

    private companion object {
        const val TIMEOUT = 10_000L
        const val ASK = "i-0000a001"
        const val REVIEW = "i-0000a002"
        const val OLD_FAILURE = "i-0000a003"
        const val OLD_PROPOSAL = "e-000000a3"
    }
}
