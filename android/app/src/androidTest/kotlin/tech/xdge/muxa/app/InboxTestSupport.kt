package tech.xdge.muxa.app

import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.junit4.ComposeTestRule
import androidx.compose.ui.test.onAllNodesWithTag
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick

/** Pane tests explicitly leave the initial phone Inbox before exercising Chat. */
@OptIn(ExperimentalTestApi::class)
fun ComposeTestRule.leaveInitialInbox() {
    fun exists(tag: String) = onAllNodesWithTag(tag).fetchSemanticsNodes().isNotEmpty()
    waitUntil(10_000) { exists("inbox-list") || exists("composer") || exists("pane") }
    if (exists("inbox-list")) onNodeWithTag("tasks-done").performClick()
}
