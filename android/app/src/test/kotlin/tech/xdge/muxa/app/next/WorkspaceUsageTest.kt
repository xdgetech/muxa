package tech.xdge.muxa.app.next

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import tech.xdge.muxa.v2.LoopChecks
import tech.xdge.muxa.v2.Terminal
import tech.xdge.muxa.v2.UsedBy
import tech.xdge.muxa.v2.WorkspaceLoop

class WorkspaceUsageTest {
    @Test
    fun lastUsedReadsAsARelativeTime() {
        val now = 1_764_000_000L
        assertEquals("never", lastUsedText(null, now))
        assertEquals("just now", lastUsedText(now - 30, now))
        assertEquals("5m ago", lastUsedText(now - 5 * 60, now))
        assertEquals("2h ago", lastUsedText(now - 2 * 3600 - 59, now))
        assertEquals("3d ago", lastUsedText(now - 3 * 86_400, now))
        assertEquals("4mo ago", lastUsedText(now - 125 * 86_400, now))
        assertEquals("1y ago", lastUsedText(now - 400 * 86_400, now))
        // A host clock ahead of the device's does not read as the future.
        assertEquals("just now", lastUsedText(now + 90, now))
    }

    @Test
    fun tasksAreNamedByTheirTerminalsTitle() {
        val terminals = listOf(Terminal("t1", "fix-login", live = true, cols = 80, rows = 24))
        val users = UsedBy(routines = listOf("nightly-backup"), tasks = listOf("t1", "t9")).named(terminals)
        assertEquals(Users(listOf("nightly-backup"), listOf("fix-login", "t9")), users)
        assertTrue(UsedBy().named(terminals).isEmpty)
    }

    @Test
    fun theLoopSaysWhichChecksItMayRun() {
        assertEquals("Not allowed", loopText(null))
        assertEquals("Allowed, any check", loopText(WorkspaceLoop(LoopChecks.Any)))
        assertEquals("Allowed: make test, make lint", loopText(WorkspaceLoop(LoopChecks.List(listOf("make test", "make lint")))))
    }
}
