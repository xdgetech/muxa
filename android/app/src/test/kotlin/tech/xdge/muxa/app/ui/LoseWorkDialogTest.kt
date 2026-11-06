package tech.xdge.muxa.app.ui

import org.junit.Assert.assertEquals
import org.junit.Test

/** ADR 0043: the host's reason starts the question, as its own sentence. */
class LoseWorkDialogTest {
    @Test
    fun theReasonBecomesTheFirstSentence() {
        assertEquals(
            "Its branch is not merged. Remove its worktree and branch anyway?",
            loseWorkMessage("its branch is not merged"),
        )
        assertEquals(
            "It has uncommitted changes. Remove its worktree and branch anyway?",
            loseWorkMessage("it has uncommitted changes."),
        )
    }
}
