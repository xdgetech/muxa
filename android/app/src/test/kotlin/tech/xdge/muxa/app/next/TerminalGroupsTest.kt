package tech.xdge.muxa.app.next

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import tech.xdge.muxa.v2.Paused
import tech.xdge.muxa.v2.Terminal
import tech.xdge.muxa.v2.TerminalStatus

class TerminalGroupsTest {
    private fun terminal(id: String, name: String? = null, live: Boolean = true, labels: Map<String, String> = emptyMap()) =
        Terminal(id, name, live = live, labels = labels, cols = 80, rows = 24)

    private fun task(id: String, workspace: String?, live: Boolean = true) = terminal(
        id, name = "Task $id", live = live,
        labels = buildMap {
            put("muxa.kind", "task"); put("muxa.role", "agent"); put("muxa.dir", "/w/$id")
            workspace?.let { put("muxa.workspace", it) }
        },
    )

    private val routineRun = terminal(
        "t5",
        labels = mapOf("muxa.kind" to "routine", "muxa.role" to "agent", "muxa.dir" to "/w/run", "muxa.routine" to "nightly-backup"),
    )

    @Test
    fun tasksGroupByWorkspaceAndOtherLiveTerminalsSitApart() {
        val groups = TerminalGroups.of(
            listOf(task("t1", "app"), terminal("t2"), task("t3", "docs"), task("t4", "app"), routineRun, task("t6", null)),
        )
        assertEquals(mapOf("app" to listOf("t1", "t4"), "docs" to listOf("t3"), null to listOf("t6")), groups.byWorkspace.mapValues { (_, v) -> v.map { it.id } })
        assertEquals(listOf("t2", "t5"), groups.others.map { it.id })
    }

    @Test
    fun terminalsThatAreNotLiveAreLeftOut() {
        val groups = TerminalGroups.of(listOf(task("t1", "app", live = false), terminal("t2", live = false)))
        assertTrue(groups.isEmpty)
    }

    @Test
    fun pausedTasksAreListedLastInTheirWorkspaceAsPaused() {
        val live = task("t1", "app").copy(status = TerminalStatus("blocked"))
        val stopped = Paused(task("t2", "app", live = false), pausedAt = 1)
        val talk = Paused(terminal("t3", live = false, labels = mapOf("muxa.kind" to "task", "muxa.dir" to "/w/t", "muxa.bot" to "b")), 1)
        val groups = TerminalGroups.of(listOf(live), listOf(stopped, talk))
        assertEquals(listOf("t1", "t2"), groups.byWorkspace.getValue("app").map { it.id })
        assertEquals(TaskState.PAUSED, groups.stateOf(stopped.terminal))
        assertEquals(TaskState.NEEDS_YOU, groups.stateOf(live))
    }

    @Test
    fun aTaskNeedsItsKindAndFolder() {
        val noFolder = terminal("t1", labels = mapOf("muxa.kind" to "task"))
        assertEquals(listOf("t1"), TerminalGroups.of(listOf(noFolder)).others.map { it.id })
    }

    @Test
    fun aTerminalIsCalledByItsNameALabelOrItsId() {
        assertEquals("shell", terminal("t3", name = "shell").title)
        assertEquals("Routine nightly-backup", routineRun.title)
        assertEquals("Terminal t3", terminal("t3").title)
        assertEquals("fix-login", terminal("t1", name = "Fix", labels = mapOf("muxa.branch" to "fix-login")).title)
    }

    @Test
    fun aBotsTalkRunAndMemberStayOffFocus() {
        val talk = terminal("t7", labels = mapOf("muxa.kind" to "task", "muxa.role" to "agent", "muxa.dir" to "/w/talk", "muxa.bot" to "pr-watch"))
        val run = terminal("t8", labels = mapOf("muxa.kind" to "routine", "muxa.role" to "agent", "muxa.dir" to "/w/scan", "muxa.bot" to "pr-watch"))
        val member = terminal("t9", labels = mapOf("muxa.kind" to "handoff", "muxa.role" to "agent", "muxa.lead" to "weekly"))
        val groups = TerminalGroups.of(listOf(task("t1", "app"), talk, run, member))
        assertEquals(listOf("t1"), groups.tasks.map { it.id })
        assertTrue(groups.others.isEmpty())
    }
}
