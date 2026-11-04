package tech.xdge.muxa.app

import android.content.Intent
import android.view.KeyEvent
import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.junit4.ComposeTestRule
import androidx.compose.ui.test.onAllNodesWithTag
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.test.core.app.ApplicationProvider
import androidx.test.platform.app.InstrumentationRegistry

/**
 * A debug demo launch in [mode] (ADR 0035). Pane tests start in Sessions mode, which shows the
 * default pane at connect; Inbox and Tasks mode stay on their list.
 */
fun demoIntent(mode: SidebarMode = SidebarMode.SESSIONS): Intent =
    Intent(ApplicationProvider.getApplicationContext<MuxaApplication>(), MainActivity::class.java)
        .putExtra(MainActivity.EXTRA_DEMO, true)
        .putExtra(MainActivity.EXTRA_MODE, mode.tag)

/** Waits for the pane that Sessions mode shows at connect: Chat's composer, or the terminal. */
@OptIn(ExperimentalTestApi::class)
fun ComposeTestRule.waitForInitialPane() {
    fun exists(tag: String) = onAllNodesWithTag(tag).fetchSemanticsNodes().isNotEmpty()
    waitUntil(10_000) { exists("composer") || exists("pane") }
}

fun ComposeTestRule.exists(tag: String) = onAllNodesWithTag(tag).fetchSemanticsNodes().isNotEmpty()

@OptIn(ExperimentalTestApi::class)
fun ComposeTestRule.waitForTag(tag: String, timeout: Long = 10_000) = waitUntilAtLeastOneExists(hasTestTag(tag), timeout)

@OptIn(ExperimentalTestApi::class)
fun ComposeTestRule.click(tag: String) {
    waitForTag(tag)
    onNodeWithTag(tag).performClick()
}

/** One step toward the sidebar: Back on a phone's stack (the pane's, an item's or a Task's), Show List beside it. */
fun ComposeTestRule.showList() {
    for (tag in listOf("back", "show-list", "item-back", "task-back")) {
        if (exists(tag)) {
            onNodeWithTag(tag).performClick()
            return
        }
    }
}

/** Picks a sidebar mode with its segmented control (ADR 0035). */
fun ComposeTestRule.pickMode(mode: SidebarMode) {
    repeat(3) { if (!exists("sidebar-mode-${mode.tag}")) showList() }
    click("sidebar-mode-${mode.tag}")
}

/** A Ctrl+Shift chord, as a keyboard case sends it; [alt] adds Alt. */
fun chord(code: Int, alt: Boolean = false) {
    val now = android.os.SystemClock.uptimeMillis()
    val meta = KeyEvent.META_CTRL_ON or KeyEvent.META_SHIFT_ON or (if (alt) KeyEvent.META_ALT_ON else 0)
    val instrumentation = InstrumentationRegistry.getInstrumentation()
    instrumentation.sendKeySync(KeyEvent(now, now, KeyEvent.ACTION_DOWN, code, 0, meta))
    instrumentation.sendKeySync(KeyEvent(now, now, KeyEvent.ACTION_UP, code, 0, meta))
}
