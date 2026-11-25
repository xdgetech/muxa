package tech.xdge.muxa.app.next

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Test
import tech.xdge.muxa.v2.FoundLogin
import tech.xdge.muxa.v2.LoopInfo
import tech.xdge.muxa.v2.Settings
import tech.xdge.muxa.v2.SettingsAssistant
import tech.xdge.muxa.v2.SettingsBot
import tech.xdge.muxa.v2.SettingsOp
import tech.xdge.muxa.v2.SettingsWorkspace
import tech.xdge.muxa.v2.SubscriptionSpec
import tech.xdge.muxa.v2.SubscriptionUsage
import tech.xdge.muxa.v2.UsageWindow

class SubscriptionDraftsTest {
    private val now = 1_000_000L

    @Test
    fun aWindowReadsAsItsShortestLimit() {
        assertEquals("5 h", PlanWindow.label(300))
        assertEquals("7 d", PlanWindow.label(10080))
        assertEquals("45 min", PlanWindow.label(45))
        val usage = SubscriptionUsage("w", "claude-code", windows = listOf(UsageWindow(300, 82, now + 60), UsageWindow(10080, 31, now + 9000)))
        assertEquals("82% of 5 h", PlanWindow.line(usage, now))
        assertEquals("5 h reset", PlanWindow.line(usage, now + 60))
        assertEquals("no data yet", PlanWindow.line(null, now))
        assertEquals("no data yet", PlanWindow.line(SubscriptionUsage("w", "codex"), now))
    }

    @Test
    fun aFoundLoginIsAddedOrAdoptedUnderItsSuggestedName() {
        val home = FoundLogin("claude-code", home = "~/.claude")
        assertEquals("claude", Found.name(home))
        assertEquals(SettingsOp.SubscriptionAdd(SubscriptionSpec("claude", "claude-code", "~/.claude")), Found.op(home))
        val env = FoundLogin("codex", workspace = "My App")
        assertEquals("codex-my-app", Found.name(env))
        assertEquals("Codex · from My App's env", Found.origin(env))
        assertEquals(SettingsOp.SubscriptionAdopt("My App", "codex", "codex-my-app"), Found.op(env))
    }

    @Test
    fun aNewSubscriptionGetsItsOwnFolderUntilOneIsTyped() {
        val d = SubscriptionDraft(name = " work ")
        assertEquals(SettingsOp.SubscriptionAdd(SubscriptionSpec("work", "claude-code", "~/.claude-work")), d.op())
        assertEquals("CLAUDE_CONFIG_DIR=~/.claude-work claude", d.command)
        assertEquals("codex login", d.copy(agent = "codex", home = "~/.codex").command)
        assertFalse(SubscriptionDraft(name = "Work Login").isValid)
        val edit = SubscriptionDraft.of(SubscriptionSpec("work", "claude-code", "~/.claude-work", "Max")).copy(plan = "")
        assertEquals(SettingsOp.SubscriptionUpdate(SubscriptionSpec("work", "claude-code", "~/.claude-work")), edit.op())
    }

    @Test
    fun startTaskNamesTheFirstLoginOfTheAgentsKind() {
        val s = settings()
        val usage = listOf(SubscriptionUsage("work", "claude-code", windows = listOf(UsageWindow(300, 82, Long.MAX_VALUE))))
        assertEquals("Runs on work · 82% of 5 h", s.runsOn("app", "claude", usage))
        assertEquals("Runs on gpt · no data yet", s.runsOn("app", "/usr/bin/codex", usage))
        assertNull(s.runsOn("other", "claude", usage))
    }

    @Test
    fun usedByNamesWorkspacesBotsAndTheAssistant() {
        assertEquals(listOf("app", "scout in app", "Capture assistant"), settings().usersOf("work"))
        assertEquals(emptyList<String>(), settings().usersOf("spare"))
    }

    @Test
    fun theAssistantSendsItsSubscriptionsOnlyWhenTheyChanged() {
        val d = AssistantDraft.of(SettingsAssistant(true, subscriptions = listOf("work")))
        assertNull(d.op().subscriptions)
        assertEquals(listOf("spare", "work"), d.copy(subscriptions = listOf("spare", "work")).op().subscriptions)
    }

    @Test
    fun aLoopHeldForALimitSaysWhy() {
        val held = LoopInfo("t1", "make test", 3, 600, "paused", 1, 1, 2, reason = "limit-reset", resumesAt = now)
        assertEquals("Paused: its subscription is at its limit", LoopText.state(held))
        assertEquals("Paused: the agent waits for you", LoopText.state(held.copy(reason = "waiting")))
    }

    private fun settings() = Settings(
        revision = "r",
        workspaces = listOf(
            SettingsWorkspace("app", "/app", subscriptions = listOf("work", "gpt"), bots = listOf(SettingsBot("scout", subscriptions = listOf("work")))),
            SettingsWorkspace("other", "/other"),
        ),
        assistant = SettingsAssistant(subscriptions = listOf("work")),
        subscriptions = listOf(
            SubscriptionSpec("work", "claude-code", "~/.claude-work"),
            SubscriptionSpec("gpt", "codex", "~/.codex"),
            SubscriptionSpec("spare", "claude-code", "~/.claude-spare"),
        ),
    )
}
