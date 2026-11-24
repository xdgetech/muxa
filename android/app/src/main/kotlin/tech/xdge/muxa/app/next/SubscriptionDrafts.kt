package tech.xdge.muxa.app.next

import tech.xdge.muxa.v2.FoundLogin
import tech.xdge.muxa.v2.Settings
import tech.xdge.muxa.v2.SettingsOp
import tech.xdge.muxa.v2.SubscriptionSpec
import tech.xdge.muxa.v2.SubscriptionUsage

// Subscriptions (ADR 0091) as the Settings screens show and save them. The host checks the rest.

/** How a plan window reads: `5 h`, `7 d`, `45 min`, and the thresholds it is colored by. */
object PlanWindow {
    const val WARN = 80
    const val SWITCH = 95

    fun label(minutes: Int): String = when {
        minutes % MINUTES_A_DAY == 0 -> "${minutes / MINUTES_A_DAY} d"
        minutes % MINUTES_AN_HOUR == 0 -> "${minutes / MINUTES_AN_HOUR} h"
        else -> "$minutes min"
    }

    /** `82% of 5 h`, `5 h reset`, or `no data yet`, from the shortest window. */
    fun line(usage: SubscriptionUsage?, nowSeconds: Long = System.currentTimeMillis() / 1000): String {
        val w = usage?.windows?.firstOrNull() ?: return "no data yet"
        if (w.hasReset(nowSeconds)) return "${label(w.minutes)} reset"
        return "${w.usedPercent}% of ${label(w.minutes)}"
    }

    private const val MINUTES_AN_HOUR = 60
    private const val MINUTES_A_DAY = 1440
}

/** How onboarding names and adds a login the host found (ADR 0091 §7). */
object Found {
    /** `claude`, `codex`, or `claude-<workspace>` for a Workspace's env login. */
    fun name(f: FoundLogin): String {
        val agent = if (f.agent == CODEX) "codex" else "claude"
        val w = f.workspace ?: return agent
        val slug = w.lowercase().map { if (it.isLetterOrDigit()) it else '-' }.joinToString("")
        return "$agent-$slug"
    }

    fun origin(f: FoundLogin): String {
        val agent = agentTitle(f.agent)
        return f.workspace?.let { "$agent · from $it's env" } ?: "$agent · ${f.home.orEmpty()} (the default login)"
    }

    fun op(f: FoundLogin): SettingsOp = f.workspace?.let { SettingsOp.SubscriptionAdopt(it, f.agent, name(f)) }
        ?: SettingsOp.SubscriptionAdd(SubscriptionSpec(name(f), f.agent, f.home.orEmpty()))
}

fun agentTitle(agent: String): String = if (agent == CODEX) "Codex" else "Claude Code"

/** A subscription's fields. A blank [home] is `~/.claude-<name>` or `~/.codex-<name>`. */
data class SubscriptionDraft(
    val existing: String? = null,
    val name: String = "",
    val agent: String = CLAUDE_CODE,
    val home: String = "",
    val plan: String = "",
) {
    val folder: String get() = home.trim().ifEmpty { "~/.${if (agent == CODEX) "codex" else "claude"}-${name.trim()}" }
    val isValid: Boolean get() = BotDraft.isValidName(name.trim())

    fun op(): SettingsOp {
        val spec = SubscriptionSpec(name.trim(), agent, folder, plan.trim().ifEmpty { null })
        return if (existing == null) SettingsOp.SubscriptionAdd(spec) else SettingsOp.SubscriptionUpdate(spec)
    }

    /** The command that logs in to this folder from a shell on the host. */
    val command: String
        get() {
            val isDefault = folder == if (agent == CODEX) "~/.codex" else "~/.claude"
            return when {
                agent == CODEX && isDefault -> "codex login"
                agent == CODEX -> "CODEX_HOME=$folder codex login"
                isDefault -> "claude"
                else -> "CLAUDE_CONFIG_DIR=$folder claude"
            }
        }

    companion object {
        fun of(s: SubscriptionSpec) = SubscriptionDraft(s.name, s.name, s.agent, s.home, s.plan.orEmpty())
    }
}

/** The host's subscriptions of [agent], or all, by name. */
fun Settings.subscriptionNames(agent: String? = null): List<String> =
    subscriptions.filter { agent == null || it.agent == agent }.map { it.name }

/** Who lists [name]: Workspaces, Bots and the Capture assistant. */
fun Settings.usersOf(name: String): List<String> =
    workspaces.filter { name in it.subscriptions }.map { it.name } +
        workspaces.flatMap { w -> w.bots.filter { name in it.subscriptions }.map { "${it.name} in ${w.name}" } } +
        listOfNotNull("Capture assistant".takeIf { assistant?.subscriptions?.contains(name) == true })

/**
 * "Runs on claude-work · 82% of 5 h": the first of [workspace]'s subscriptions for its [agent] command,
 * as the host picks it. Null when the Workspace lists none of that kind.
 */
fun Settings.runsOn(workspace: String, agent: String?, usage: List<SubscriptionUsage>): String? {
    val names = workspaces.firstOrNull { it.name == workspace }?.subscriptions.orEmpty()
    val kind = if (agent?.endsWith("codex") == true) CODEX else CLAUDE_CODE
    val name = names.firstOrNull { n -> subscriptions.any { it.name == n && it.agent == kind } } ?: return null
    return "Runs on $name · ${PlanWindow.line(usage.firstOrNull { it.name == name })}"
}

private const val CLAUDE_CODE = WorkerDraft.CLAUDE_CODE
private const val CODEX = WorkerDraft.CODEX
