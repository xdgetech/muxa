package tech.xdge.muxa.session

import tech.xdge.muxa.protocol.Agent
import tech.xdge.muxa.protocol.AgentState
import tech.xdge.muxa.protocol.Pane
import tech.xdge.muxa.protocol.Role
import tech.xdge.muxa.protocol.Session
import tech.xdge.muxa.protocol.TaskRecord
import tech.xdge.muxa.protocol.TaskStatus

/** The Inbox groups, in the order shown. Titles match the iOS app. */
enum class InboxGroup(val title: String) {
    NEEDS_ATTENTION("Needs attention"),
    READY_FOR_REVIEW("Ready for review"),
    RUNNING("Running"),
    NO_AGENT("No agent"),
    CLOSED("Closed"),
}

enum class InboxNeed(val wire: String) {
    NONE("none"), ANSWER("answer"), AGENT_ERROR("agent-error"), LOOP("loop"), REVIEW("review"),
    NO_AGENT("no-agent"), NO_SESSION("no-session"),
}

/**
 * One Task with what runs it now. [agentPane] and [agent] are null when no adapter
 * reports a state that can be believed (invariant 6).
 */
data class InboxItem(
    val record: TaskRecord,
    val group: InboxGroup,
    val session: Session? = null,
    val agentPane: Pane? = null,
    val agent: Agent? = null,
)

val InboxItem.need: InboxNeed
    get() = when {
        group == InboxGroup.CLOSED -> InboxNeed.NONE
        agent?.state == AgentState.WAITING -> InboxNeed.ANSWER
        agent?.state == AgentState.ERROR -> InboxNeed.AGENT_ERROR
        record.loop?.needsUser == true -> InboxNeed.LOOP
        group == InboxGroup.RUNNING -> InboxNeed.NONE
        group == InboxGroup.READY_FOR_REVIEW -> InboxNeed.REVIEW
        session == null -> InboxNeed.NO_SESSION
        else -> InboxNeed.NO_AGENT
    }

fun triageQueue(records: List<TaskRecord>, sessions: List<Session>): List<InboxItem> =
    inbox(records, sessions).flatMap { it.items }.filter { it.need != InboxNeed.NONE }

data class InboxSection(val group: InboxGroup, val items: List<InboxItem>)

val TaskStatus.isClosed: Boolean get() = this == TaskStatus.DONE || this == TaskStatus.DROPPED

/** The session that runs [record]: the one whose `taskId` is its id. */
fun List<Session>.runningTask(record: TaskRecord): Session? = firstOrNull { it.taskId == record.id }

/** The pane whose agent state stands for the session: the tagged agent, else the first pane with a state. */
val Session.agentPane: Pane?
    get() = visiblePanes.firstOrNull { it.role == Role.AGENT } ?: visiblePanes.firstOrNull { it.agent != null }

/** Where [record] belongs, from its status, its Task Loop and its session's agent state (ADR 0015, 0019). */
fun inboxItem(record: TaskRecord, sessions: List<Session>): InboxItem {
    if (record.status.isClosed) return InboxItem(record, InboxGroup.CLOSED, sessions.runningTask(record))
    val item = agentItem(record, sessions)
    return if (record.loop?.needsUser == true) item.copy(group = InboxGroup.NEEDS_ATTENTION) else item
}

private fun agentItem(record: TaskRecord, sessions: List<Session>): InboxItem {
    val session = sessions.runningTask(record) ?: return InboxItem(record, InboxGroup.NO_AGENT)
    val pane = session.agentPane
    val agent = pane?.agent
    val state = agent?.shown(pane.shellForeground)
        ?: return InboxItem(record, InboxGroup.NO_AGENT, session, pane)
    val group = when (state) {
        // An error is the adapter's word too, and it needs the user.
        AgentState.WAITING, AgentState.ERROR -> InboxGroup.NEEDS_ATTENTION
        AgentState.RUNNING -> InboxGroup.RUNNING
        // A Task already in review, or with a status this app does not know, has no better group.
        AgentState.IDLE, AgentState.DONE -> InboxGroup.READY_FOR_REVIEW
    }
    return InboxItem(record, group, session, pane, agent)
}

/** The non-empty groups in [InboxGroup] order; newest change first within a group. */
fun inbox(records: List<TaskRecord>, sessions: List<Session>): List<InboxSection> {
    val byGroup = records.map { inboxItem(it, sessions) }.groupBy { it.group }
    return InboxGroup.entries.mapNotNull { group ->
        val items = byGroup[group] ?: return@mapNotNull null
        InboxSection(group, items.sortedWith(compareByDescending<InboxItem> { it.record.updatedAt }.thenByDescending { it.record.id }))
    }
}

/** Conservative local snooze identity; it is not an approval token. */
val InboxItem.attentionKey: String get() = listOf(
    record.id, record.rev, session?.id, agentPane?.id, need.wire, agent?.state, agent?.since,
    record.loop?.state, record.loop?.reason, record.loop?.since,
).joinToString("\u001f")
