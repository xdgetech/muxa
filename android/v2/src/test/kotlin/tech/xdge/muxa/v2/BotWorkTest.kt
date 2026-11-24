package tech.xdge.muxa.v2

import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import kotlinx.serialization.Serializable
import kotlinx.serialization.SerialName
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.decodeFromJsonElement
import kotlinx.serialization.json.jsonObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** A Bot's work grouped by PROTOCOL-v2.md, "A Bot's work" (ADR 0086), against Swift's fixture `bot-work.json`. */
class BotWorkTest {
    @Serializable
    private data class Case(
        val bot: String,
        val workspace: String,
        val enabled: Boolean,
        val items: List<String>,
        val terminals: List<String>,
        @SerialName("needs_you") val needsYou: List<String>,
        @SerialName("in_progress") val inProgress: List<String>,
        val planned: List<String>,
        val recent: List<String>,
        val status: String,
    )

    private val fixture: JsonObject = json.parseToJsonElement(
        javaClass.classLoader!!.getResource("bot-work.json")!!.readText(),
    ).jsonObject

    private inline fun <reified T> field(name: String): T = json.decodeFromJsonElement(fixture.getValue(name))

    @Test
    fun eachCaseGroupsAsTheProtocolSays() {
        val inbox = field<Inbox>("inbox")
        val terminals = field<List<Terminal>>("terminals")
        val routines = field<List<Routine>>("routines")
        val cases = field<List<Case>>("cases")
        assertEquals(7, cases.size)
        for (c in cases) {
            val w = BotWork.of(c.bot, c.workspace, c.enabled, inbox, terminals, routines)
            val name = "${c.workspace}/${c.bot}"
            assertEquals("$name items", c.items, w.items.map { it.id })
            assertEquals("$name terminals", c.terminals, w.terminals.map { it.id })
            assertEquals("$name needs you", c.needsYou, w.needsYou.map { it.id })
            assertEquals("$name in progress", c.inProgress, w.inProgress.map { it.id })
            assertEquals("$name planned", c.planned, w.planned.map { it.name })
            assertEquals("$name recent", c.recent, w.recent.map { it.id })
            assertEquals("$name status", c.status, w.status.wire)
        }
    }

    @Test
    fun memoryHistoryDecodesAndIsOptional() {
        val bot = json.decodeFromString<SettingsBot>(
            """{"name":"b","enabled":true,"memory_versions":2,"memory_history":[{"version":1,"at":200},{"version":2,"at":100}]}""",
        )
        assertEquals(listOf(MemoryVersion(1, 200), MemoryVersion(2, 100)), bot.memoryHistory)
        val old = json.decodeFromString<SettingsBot>("""{"name":"b","enabled":true}""")
        assertTrue(old.memoryHistory.isEmpty())
        assertFalse("memory_history" in json.encodeToString(SettingsBot.serializer(), old))
    }

    @Test
    fun aTalkARunAndAMemberBelongToABotAndAPlainTaskDoesNot() {
        fun t(vararg labels: Pair<String, String>) = Terminal("t", live = true, labels = mapOf(*labels), cols = 80, rows = 24)
        val member = t("muxa.kind" to "handoff", "muxa.role" to "agent", "muxa.bot" to "writer", "muxa.lead" to "weekly")
        assertEquals("weekly" to "writer", member.lead to member.bot)
        assertTrue(t("muxa.kind" to "task", "muxa.role" to "agent", "muxa.bot" to "pr-watch").belongsToBot)
        assertTrue(t("muxa.kind" to "routine", "muxa.role" to "agent", "muxa.bot" to "pr-watch").belongsToBot)
        assertTrue(t("muxa.kind" to "handoff", "muxa.role" to "agent", "muxa.lead" to "weekly").belongsToBot)
        assertFalse(t("muxa.kind" to "task", "muxa.role" to "agent").belongsToBot)
    }

    @Test
    fun theDemoHasATalkWithItsProposalAndARunOfPrWatch() = runBlocking {
        withTimeout(10_000) {
            val c = V2Client.open(DemoChannel(), "test")
            val inbox = c.inbox()
            val (routines, _) = c.routines()
            val bot = c.settings().workspaces.first { it.name == "app" }.bots.single()
            val w = BotWork.of("pr-watch", "app", bot.enabled, inbox, c.terminals(), routines)
            assertEquals(listOf("t7", "t8"), w.terminals.map { it.id })
            assertEquals("t7", inbox.items.first { it.id == "d12" }.terminal)
            assertEquals(listOf("scan"), w.planned.map { it.name })
            assertEquals(BotWork.Status.NEEDS_YOU, w.status)
            assertEquals(listOf(1, 2), bot.memoryHistory.map { it.version })
            c.close()
        }
    }
}
