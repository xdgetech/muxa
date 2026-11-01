package tech.xdge.muxa.session

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import tech.xdge.muxa.protocol.Agent
import tech.xdge.muxa.protocol.AgentState
import tech.xdge.muxa.protocol.Pane
import tech.xdge.muxa.protocol.Role
import tech.xdge.muxa.protocol.Session
import tech.xdge.muxa.protocol.TaskRecord
import tech.xdge.muxa.protocol.TaskStatus

class InboxTest {
    private fun record(id: String, status: TaskStatus = TaskStatus.OPEN, updatedAt: Long = 1) =
        TaskRecord(id, 1, "work", "Title $id", "Goal", status, 1, updatedAt)

    private fun pane(
        id: String,
        role: Role = Role.AGENT,
        state: AgentState? = null,
        shellForeground: Boolean = false,
    ) = Pane(id, "@1", 0, role, 80, 24, "claude", "/w", false, shellForeground, state?.let { Agent(it, 100) })

    private fun session(taskId: String?, vararg panes: Pane) =
        Session(id = "$" + (taskId ?: "x"), name = "s-$taskId", taskId = taskId, panes = panes.toList())

    private fun group(record: TaskRecord, vararg sessions: Session) = inboxItem(record, sessions.toList()).group

    @Test
    fun theAgentStateDecidesTheGroupOfAnOpenTask() {
        val r = record("t-1")
        assertEquals(InboxGroup.NEEDS_ATTENTION, group(r, session("t-1", pane("%1", state = AgentState.WAITING))))
        assertEquals(InboxGroup.READY_FOR_REVIEW, group(r, session("t-1", pane("%1", state = AgentState.IDLE))))
        assertEquals(InboxGroup.READY_FOR_REVIEW, group(r, session("t-1", pane("%1", state = AgentState.DONE))))
        assertEquals(InboxGroup.RUNNING, group(r, session("t-1", pane("%1", state = AgentState.RUNNING))))
        assertEquals(InboxGroup.NEEDS_ATTENTION, group(r, session("t-1", pane("%1", state = AgentState.ERROR))))
    }

    @Test
    fun noBelievableStateIsNoAgent() {
        val r = record("t-1")
        // No session at all, a session of another Task, no state, and a shell in front.
        assertEquals(InboxGroup.NO_AGENT, group(r))
        assertEquals(InboxGroup.NO_AGENT, group(r, session("t-2", pane("%1", state = AgentState.RUNNING))))
        assertEquals(InboxGroup.NO_AGENT, group(r, session("t-1", pane("%1"))))
        assertEquals(
            InboxGroup.NO_AGENT,
            group(r, session("t-1", pane("%1", state = AgentState.RUNNING, shellForeground = true))),
        )
        val item = inboxItem(r, listOf(session("t-1", pane("%1", state = AgentState.RUNNING, shellForeground = true))))
        assertNull(item.agent)
    }

    @Test
    fun aClosedStatusWinsOverTheAgent() {
        val waiting = session("t-1", pane("%1", state = AgentState.WAITING))
        assertEquals(InboxGroup.CLOSED, group(record("t-1", TaskStatus.DONE), waiting))
        assertEquals(InboxGroup.CLOSED, group(record("t-1", TaskStatus.DROPPED), waiting))
    }

    @Test
    fun aTaskInReviewOrWithAnUnknownStatusStaysReadyForReview() {
        val idle = session("t-1", pane("%1", state = AgentState.IDLE))
        assertEquals(InboxGroup.READY_FOR_REVIEW, group(record("t-1", TaskStatus.REVIEW), idle))
        assertEquals(InboxGroup.READY_FOR_REVIEW, group(record("t-1", TaskStatus("later")), idle))
    }

    @Test
    fun theTaggedAgentPaneSpeaksForTheSession() {
        val s = session("t-1", pane("%2", Role.SHELL, AgentState.WAITING), pane("%1", state = AgentState.RUNNING))
        val item = inboxItem(record("t-1"), listOf(s))
        assertEquals("%1", item.agentPane?.id)
        assertEquals(InboxGroup.RUNNING, item.group)
    }

    @Test
    fun groupsComeInAFixedOrderNewestFirst() {
        val sections = inbox(
            listOf(
                record("t-1", TaskStatus.DONE),
                record("t-2", updatedAt = 1),
                record("t-3", updatedAt = 5),
                record("t-4"),
            ),
            listOf(session("t-4", pane("%1", state = AgentState.WAITING))),
        )
        assertEquals(listOf(InboxGroup.NEEDS_ATTENTION, InboxGroup.NO_AGENT, InboxGroup.CLOSED), sections.map { it.group })
        assertEquals(listOf("t-3", "t-2"), sections[1].items.map { it.record.id })
        assertEquals(
            listOf("Needs attention", "Ready for review", "Running", "No agent", "Closed"),
            InboxGroup.entries.map { it.title },
        )
    }
}
