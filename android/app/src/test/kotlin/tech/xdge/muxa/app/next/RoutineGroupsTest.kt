package tech.xdge.muxa.app.next

import org.junit.Assert.assertEquals
import org.junit.Test
import tech.xdge.muxa.v2.Routine

class RoutineGroupsTest {
    @Test
    fun routinesGroupByBotWithTheRestUnderOther() {
        fun r(name: String, bot: String? = null) = Routine("app", name, enabled = true, bot = bot)
        val plain = listOf(r("digest"), r("backup"))
        assertEquals(listOf(RoutineGroup(null, plain)), routineGroups(plain))
        val routines = listOf(r("digest"), r("scan", "pr-watch"), r("triage", "issues"), r("backup", "pr-watch"))
        val groups = routineGroups(routines)
        assertEquals(
            listOf(
                RoutineGroup("issues", listOf(r("triage", "issues"))),
                RoutineGroup("pr-watch", listOf(r("scan", "pr-watch"), r("backup", "pr-watch"))),
                RoutineGroup(null, listOf(r("digest"))),
            ),
            groups,
        )
    }
}
