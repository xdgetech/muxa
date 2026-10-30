package tech.xdge.muxa.session

import kotlinx.serialization.Serializable
import tech.xdge.muxa.protocol.Agent
import tech.xdge.muxa.protocol.AgentState
import tech.xdge.muxa.protocol.Pane
import tech.xdge.muxa.protocol.Role
import tech.xdge.muxa.protocol.Session

// Which pane the Agent/Terminal toggle shows (PROTOCOL.md, "Agent/Terminal toggle").

val Session.visiblePanes: List<Pane> get() = panes.filter { it.role != Role.HIDDEN }

/** The pane to open first: the agent, else the first visible pane. */
val Session.defaultPane: Pane? get() = visiblePanes.firstOrNull { it.role == Role.AGENT } ?: visiblePanes.firstOrNull()

/** The other side of the toggle for [paneId], or null when the toggle is disabled. */
fun Session.counterpart(paneId: String): Pane? {
    val pane = panes.firstOrNull { it.id == paneId } ?: return null
    return when (pane.role) {
        Role.AGENT -> visiblePanes.firstOrNull { it.role == Role.SHELL }
        Role.SHELL -> visiblePanes.firstOrNull { it.role == Role.AGENT }
        Role.HIDDEN -> null
        // No tags: the shell in the same window is the terminal.
        Role.UNKNOWN -> visiblePanes.firstOrNull {
            it.window == pane.window && it.id != pane.id && it.shellForeground != pane.shellForeground
        }
    }
}

/**
 * The pane last shown on a host. The session is kept by name: the server renumbers
 * its ids when it restarts, and a pane id alone may then be another's.
 */
@Serializable
data class LastPane(val session: String, val pane: String)

/**
 * The pane to reopen for [last]: that pane while it is still in that session, else
 * the session's default pane. Null once the session is gone.
 */
fun List<Session>.resuming(last: LastPane): Pane? {
    val session = firstOrNull { it.name == last.session } ?: return null
    return session.visiblePanes.firstOrNull { it.id == last.pane } ?: session.defaultPane
}

/** The state to show. A shell in the foreground means the runner's state is likely stale. */
fun Agent.shown(shellForeground: Boolean): AgentState? = if (shellForeground) null else state

/** Sessions of one repo, as the agent runner tags them (`@agent_repo`). Null repo: untagged. */
data class RepoGroup(val repo: String?, val sessions: List<Session>)

/**
 * Groups by repo, in the order each repo first appears in the list, with untagged
 * sessions last. Within a group, the list's order is kept.
 */
fun List<Session>.groupedByRepo(): List<RepoGroup> {
    val groups = linkedMapOf<String, MutableList<Session>>()
    val untagged = mutableListOf<Session>()
    for (session in this) {
        val repo = session.repo
        if (repo.isNullOrEmpty()) untagged += session else groups.getOrPut(repo) { mutableListOf() } += session
    }
    return groups.map { RepoGroup(it.key, it.value) } + if (untagged.isEmpty()) emptyList() else listOf(RepoGroup(null, untagged))
}

/** A repo the host already has sessions for. [dir] is the main checkout, absolute. */
data class KnownRepo(val name: String, val dir: String)

/** Each repo once, in the order its first session appears. */
val List<Session>.knownRepos: List<KnownRepo>
    get() {
        val seen = mutableSetOf<String>()
        return mapNotNull { s ->
            val dir = s.repoDir
            if (dir.isNullOrEmpty() || !seen.add(dir)) return@mapNotNull null
            KnownRepo(s.repo ?: dir.split('/').lastOrNull { it.isNotEmpty() } ?: dir, dir)
        }
    }

/** An agent elsewhere that wants attention. */
data class AgentAlert(val session: String, val sessionName: String, val pane: String, val kind: Kind) {
    enum class Kind(val message: String) {
        WAITING("needs your input"),
        DONE("finished"),
        FAILED("failed"),
    }

    val id: String get() = "$pane:$kind"
}

/**
 * What changed for the agents between [old] and this list, worth telling: an agent
 * now waiting for input, finished, or failed. The shown session is left out, and so
 * are panes [old] did not have, e.g. on the first list.
 */
fun List<Session>.alerts(old: List<Session>, shown: String?): List<AgentAlert> {
    val before = mutableMapOf<String, AgentState>()
    val known = mutableSetOf<String>()
    for (pane in old.flatMap { it.panes }) {
        known += pane.id
        alertState(pane)?.let { before[pane.id] = it }
    }
    return flatMap { s ->
        if (s.id == shown) return@flatMap emptyList()
        s.panes.mapNotNull { pane ->
            val now = alertState(pane) ?: return@mapNotNull null
            if (pane.id !in known || now == before[pane.id]) return@mapNotNull null
            val kind = alertKind(before[pane.id], now) ?: return@mapNotNull null
            AgentAlert(s.id, s.name, pane.id, kind)
        }
    }
}

/** A state is not believed while a shell is in front: the agent likely exited. */
private fun alertState(pane: Pane) = pane.agent?.shown(pane.shellForeground)

/** Running to idle is how an agent without a "done" state ends its turn. */
private fun alertKind(old: AgentState?, new: AgentState): AgentAlert.Kind? = when (new) {
    AgentState.WAITING -> AgentAlert.Kind.WAITING
    AgentState.DONE -> AgentAlert.Kind.DONE
    AgentState.ERROR -> AgentAlert.Kind.FAILED
    AgentState.IDLE -> if (old == AgentState.RUNNING) AgentAlert.Kind.DONE else null
    AgentState.RUNNING -> null
}
