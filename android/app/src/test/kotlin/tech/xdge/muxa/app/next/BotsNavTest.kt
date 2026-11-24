package tech.xdge.muxa.app.next

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import tech.xdge.muxa.v2.Inbox
import tech.xdge.muxa.v2.Routine
import tech.xdge.muxa.v2.RoutineLast
import tech.xdge.muxa.v2.Settings
import tech.xdge.muxa.v2.SettingsBot
import tech.xdge.muxa.v2.SettingsOp
import tech.xdge.muxa.v2.SettingsRoutine
import tech.xdge.muxa.v2.SettingsWorkspace
import tech.xdge.muxa.v2.Terminal
import tech.xdge.muxa.v2.TerminalStatus

class BotsNavTest {
    private fun t(id: String, vararg labels: Pair<String, String>) =
        Terminal(id, live = true, labels = mapOf("muxa.workspace" to "app", *labels), cols = 80, rows = 24)

    @Test
    fun routinesWithoutABotListFailedAndHeldOnesFirst() {
        fun r(name: String, bot: String? = null, ok: Boolean? = null, enabled: Boolean = true) =
            Routine("app", name, enabled = enabled, bot = bot, last = ok?.let { RoutineLast(1, it) })
        val routines = listOf(r("digest", ok = true), r("scan", "pr-watch"), r("backup", ok = false), r("held", enabled = false))
        assertEquals(listOf("backup", "held", "digest"), routinesWithoutBot(routines).map { it.name })
    }

    // What RoutineGroupsTest covered for the old Routines tab: each Bot gets its own Routines, and the rest
    // are "Routines without a Bot". A Bot's Routines show on its page (BotWork.planned and its Routines tab).
    @Test
    fun routinesGoToTheirBotAndTheRestAreWithoutABot() {
        fun r(name: String, bot: String? = null) = Routine("app", name, enabled = true, nextDue = 10, bot = bot)
        val routines = listOf(r("digest"), r("scan", "pr-watch"), r("triage", "issues"), r("backup", "pr-watch"))
        val ws = SettingsWorkspace("app", "/w", bots = listOf(SettingsBot("issues"), SettingsBot("pr-watch")))
        val entries = botEntries(Settings("1", workspaces = listOf(ws)), Inbox.EMPTY, emptyList(), routines)
        assertEquals(
            mapOf("issues" to listOf("triage"), "pr-watch" to listOf("scan", "backup")),
            entries.associate { e -> e.name to e.work.planned.map { it.name } },
        )
        assertEquals(listOf("digest"), routinesWithoutBot(routines).map { it.name })
    }

    @Test
    fun aTerminalOpensOnItsOwnBotsPageOnItsChat() {
        val nav = BotNav()
        nav.open(t("t6", "muxa.kind" to "handoff", "muxa.bot" to "writer", "muxa.lead" to "weekly"))
        assertEquals(BotPick("app", "writer") to "t6", nav.pick to nav.terminal)
        assertEquals(BotTab.CHATS, nav.tab)
        assertTrue(nav.back())
        assertNull(nav.terminal)
        assertTrue(nav.back())
        assertNull(nav.pick)
        assertFalse(nav.back())
    }

    @Test
    fun aBotsTalksAreItsTaskAgentsAndTheRestAreRuns() {
        val talk = t("t7", "muxa.kind" to "task", "muxa.role" to "agent", "muxa.bot" to "pr-watch")
        val run = t("t8", "muxa.kind" to "routine", "muxa.role" to "agent", "muxa.bot" to "pr-watch", "muxa.routine" to "scan")
        val ws = SettingsWorkspace("app", "/w", bots = listOf(SettingsBot("pr-watch")))
        val e = botEntries(Settings("1", workspaces = listOf(ws)), Inbox.EMPTY, listOf(talk, run), emptyList()).single()
        assertEquals(listOf("t7") to listOf("t8"), e.talks.map { it.id } to e.runs.map { it.id })
        assertEquals("chat" to "run of scan", talk.botRole("pr-watch") to run.botRole("pr-watch"))
    }

    @Test
    fun removingABotMovesItsRoutinesInTheSameUpdate() {
        val ws = SettingsWorkspace(
            "app", "/w",
            routines = listOf(SettingsRoutine("scan", instructions = "s.md", bot = "pr-watch"), SettingsRoutine("other", instructions = "o.md")),
        )
        val ops = ws.botRemoval("pr-watch", "triage")
        assertEquals(2, ops.size)
        assertEquals("triage", (ops[0] as SettingsOp.RoutineUpdate).routine.bot)
        assertEquals(SettingsOp.BotRemove("app", "pr-watch"), ops[1])
    }

    private fun talk(id: String, state: String?, live: Boolean = true) = Terminal(
        id, live = live, status = state?.let { TerminalStatus(it) }, cols = 80, rows = 24,
        labels = mapOf("muxa.workspace" to "app", "muxa.kind" to "task", "muxa.role" to "agent", "muxa.bot" to "pr-watch"),
    )

    @Test
    fun aBotLandsOnItsNewestLiveTalkEvenWhenItsTurnIsDone() {
        assertEquals("t3", liveTalk(listOf(talk("t1", "working"), talk("t2", "done"), talk("t3", "waiting"))).let { it?.id })
        assertEquals("t2", liveTalk(listOf(talk("t1", null), talk("t2", "done"))).let { it?.id })
        assertNull(liveTalk(listOf(talk("t2", "working", live = false))))
        assertNull(liveTalk(emptyList()))
    }

    private fun entry(vararg terminals: Terminal): BotEntry {
        val ws = SettingsWorkspace("app", "/w", bots = listOf(SettingsBot("pr-watch")))
        return botEntries(Settings("1", workspaces = listOf(ws)), Inbox.EMPTY, terminals.toList(), emptyList()).single()
    }

    @Test
    fun aPickLandsOnTheLiveTalkElseOnTheLastTabElseHome() {
        val nav = BotNav()
        nav.pickBot(entry())
        assertEquals(BotTab.HOME to null, nav.tab to nav.terminal)
        nav.selectTab(BotTab.FILES)
        nav.back()
        nav.pickBot(entry())
        assertEquals("a Bot with no talk keeps its last tab", BotTab.FILES to null, nav.tab to nav.terminal)
        nav.back()
        nav.pickBot(entry(talk("t2", "done")))
        assertEquals("a talk waiting for the user wins over the last tab", BotTab.CHATS to "t2", nav.tab to nav.terminal)
    }

    @Test
    fun aTalkOpenedFromElsewhereIsNotTheFirstPick() {
        val nav = BotNav()
        val t = talk("t9", "working")
        nav.open(t)
        nav.back()
        nav.back()
        nav.pickBot(entry(t))
        assertEquals(BotTab.CHATS to "t9", nav.tab to nav.terminal)
    }
}
