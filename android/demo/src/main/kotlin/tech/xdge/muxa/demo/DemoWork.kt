package tech.xdge.muxa.demo

import tech.xdge.muxa.protocol.Agent
import tech.xdge.muxa.protocol.AgentState
import tech.xdge.muxa.protocol.ErrorCode
import tech.xdge.muxa.protocol.InboxAction
import tech.xdge.muxa.protocol.InboxEntry
import tech.xdge.muxa.protocol.Pane
import tech.xdge.muxa.protocol.Proposal
import tech.xdge.muxa.protocol.Role
import tech.xdge.muxa.protocol.RoutineFailure
import tech.xdge.muxa.protocol.RoutineInfo
import tech.xdge.muxa.protocol.RoutineRun
import tech.xdge.muxa.protocol.RoutineSuccess
import tech.xdge.muxa.protocol.RoutineTrigger
import tech.xdge.muxa.protocol.Session
import tech.xdge.muxa.protocol.SourceLink
import java.security.MessageDigest

/**
 * The demo bridge's Routines and stored Inbox entries (ADR 0026), with the host's checks:
 * one run per Routine, approve once, no action on a closed entry. The iOS demo serves the
 * same ones. Not thread safe: the bridge holds its lock.
 */
internal class DemoWork(
    var routines: List<RoutineInfo>,
    private var entries: List<InboxEntry>,
    /** The sessions of runs that are open at the start. */
    val sessions: List<Session>,
) {
    class Refusal(val code: ErrorCode, message: String) : Exception(message)

    /** What [update] did: the entry, and for a first approval the goal of the Task to create. */
    class Update(val entry: InboxEntry, val goal: String?)

    /** The entries as `inbox-list` sends them: a Later whose time passed is open again. */
    fun shownEntries(now: Long): List<InboxEntry> = entries.map {
        if (it.disposition == "later" && (it.until ?: 0) <= now) it.copy(disposition = "open", until = null) else it
    }

    /** Starts a run now: the Routine, and the run's session numbered [n], whose agent runs. */
    fun run(workspace: String, routine: String, n: Int, now: Long): Pair<RoutineInfo, Session> {
        val current = routines.firstOrNull { it.workspace == workspace && it.name == routine }
            ?: throw Refusal(ErrorCode.BAD_REQUEST, "$workspace/$routine is not a routine")
        if (!current.enabled) throw Refusal(ErrorCode.BAD_REQUEST, "$workspace/$routine is disabled")
        if (current.running != null) throw Refusal(ErrorCode.CONFLICT, "$workspace/$routine already runs")
        val task = "t-run%03d".format(n)
        val started = current.copy(running = RoutineRun("now-$now", now, task))
        routines = routines.map { if (it === current) started else it }
        return started to runSession(routine, task, n, AgentState.RUNNING, now)
    }

    /**
     * Dismiss and Later change the entry. Approve of an open proposal returns the goal of the
     * Task to create; [approved] then records it. Approving again returns the entry unchanged.
     */
    fun update(id: String, action: InboxAction, until: Long?, now: Long): Update {
        val i = entries.indexOfFirst { it.id == id }
        if (i < 0) throw Refusal(ErrorCode.BAD_REQUEST, "$id is not an entry")
        val entry = shownEntries(now)[i]
        if (action == InboxAction.APPROVE && entry.disposition == "approved") return Update(entry, null)
        if (entry.disposition != "open" && entry.disposition != "later") {
            throw Refusal(ErrorCode.BAD_REQUEST, "$id is ${entry.disposition}")
        }
        val saved = when (action) {
            InboxAction.APPROVE -> {
                val p = entry.proposal
                if (p == null || entry.kind != "proposal") throw Refusal(ErrorCode.BAD_REQUEST, "a briefing has nothing to approve")
                return Update(entry, "${p.action}\nTarget: ${p.target}\n\n${p.content}\n\nmuxa-proposal: ${entry.id} ${p.hash}")
            }
            InboxAction.DISMISS -> entry.copy(disposition = "dismissed", until = null)
            InboxAction.LATER -> {
                if (until == null || until <= now || until > now + LATER_LIMIT) {
                    throw Refusal(ErrorCode.BAD_REQUEST, "until must be in the future and at most 366 days ahead")
                }
                entry.copy(disposition = "later", until = until)
            }
            else -> throw Refusal(ErrorCode.BAD_REQUEST, "$action is not an entry's action")
        }
        entries = entries.map { if (it.id == id) saved else it }
        return Update(saved, null)
    }

    fun approved(id: String, task: String): InboxEntry {
        val saved = entries.first { it.id == id }.copy(disposition = "approved", task = task, until = null)
        entries = entries.map { if (it.id == id) saved else it }
        return saved
    }

    companion object {
        private const val LATER_LIMIT = 366L * 86_400

        /** The UI tests' Routines: one with a run that waits on the user, so its need shows in Inbox. */
        fun tests(now: Long) = DemoWork(
            routines(now, running = "t-r00002"), entries(now),
            listOf(runSession("linear-triage", "t-r00002", 90, AgentState.WAITING, now - 120)),
        )

        /** The same without the waiting run, so the store screenshots show only decisions. */
        fun showcase(now: Long) = DemoWork(routines(now, running = null), entries(now), emptyList())

        private fun runSession(routine: String, task: String, n: Int, state: AgentState, since: Long) = Session(
            id = "$$n", name = "$routine-$task", taskId = task,
            panes = listOf(
                Pane(
                    "%$n", "@$n", 0, Role.AGENT, 100, 30, "claude", "/work/routines/$routine-$task",
                    dead = false, shellForeground = false, agent = Agent(state, since),
                ),
            ),
        )

        fun hash(action: String, target: String, content: String): String {
            val digest = MessageDigest.getInstance("SHA-256").digest("$action\u0000$target\u0000$content".toByteArray())
            return "sha256:" + digest.joinToString("") { "%02x".format(it) }
        }

        private fun entry(
            id: String, routine: String, kind: String, title: String, body: String, createdAt: Long,
            links: List<SourceLink> = emptyList(), proposal: Triple<String, String, String>? = null,
        ) = InboxEntry(
            id, "work", routine, "$routine:$id", kind, title, body, links, createdAt, run = "t-r00001",
            occurrence = "2026-11-03T08:00", disposition = "open",
            proposal = proposal?.let { (action, target, content) -> Proposal(action, target, content, hash(action, target, content)) },
        )

        private fun routines(now: Long, running: String?) = listOf(
            RoutineInfo(
                "work", "slack-digest", RoutineTrigger("08:00", "Europe/Berlin"), enabled = true, project = "App",
                nextDue = now + 20 * 3_600,
                lastSuccess = RoutineSuccess(now - 4 * 3_600, "2026-11-03T08:00", "t-r00001", 3),
                lastFailure = RoutineFailure(
                    now - 2 * 86_400, "2026-11-01T08:00", "t-r00000", "The run t-r00000 ended without .muxa/routine/output.json.",
                ),
            ),
            RoutineInfo(
                "work", "linear-triage", RoutineTrigger("09:30", "Asia/Tokyo"), enabled = true, project = "App",
                nextDue = now + 22 * 3_600,
                running = running?.let { RoutineRun("2026-11-03T09:30", now - 300, it) },
            ),
            RoutineInfo("personal", "weekly-memory", enabled = false),
        )

        /** Newest first, as the host lists them. */
        private fun entries(now: Long) = listOf(
            entry(
                "e-000000a1", "slack-digest", "briefing", "Release moved to Thursday",
                "The release thread agreed to ship on Thursday after the QA pass.", now - 600,
                links = listOf(SourceLink("https://example.slack.com/archives/C01/p1762150000", "Release thread")),
            ),
            entry(
                "e-000000a2", "linear-triage", "proposal", "Close the stale issue ENG-12",
                "No activity for 30 days, and the fix shipped in 1.4.", now - 900,
                proposal = Triple("Close the issue", "linear:ENG-12", "Closing: no activity for 30 days, fixed in 1.4."),
            ),
            entry(
                "e-000000a3", "slack-digest", "proposal", "Answer the question about the beta",
                "Dana asked when the beta opens.", now - 1_200,
                proposal = Triple("Reply in the thread", "slack:C01:1762150100", "The beta opens next Monday."),
            ),
        )
    }
}
