package tech.xdge.muxa.session

import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test
import tech.xdge.muxa.protocol.AssistantInfo
import tech.xdge.muxa.protocol.Message
import tech.xdge.muxa.protocol.Pane
import tech.xdge.muxa.protocol.Request
import tech.xdge.muxa.protocol.Role
import tech.xdge.muxa.protocol.Session

/** Talk to Assistant: the overall assistant's root Ask session (ADR 0054). */
class AskRootTest {
    /** A host with an overall workspace that opens its root Ask session at `work` level 15. */
    private class RootBridge(level: Int) : FakeBridge() {
        init {
            capabilities = listOf("task-records", "work:$level")
        }

        override fun answer(req: Request) {
            when {
                req is Request.ListInbox -> {
                    emit(Message.InboxList(req.id, emptyList(), assistant = AssistantInfo("secretary")))
                    emit(Message.Result(req.id))
                }
                req is Request.StartSession && req.root -> {
                    if (sessions.none { it.id == "$8" }) {
                        sessions = sessions + Session(
                            id = "$8", name = "ask-${req.workspace}", ask = req.workspace,
                            panes = listOf(Pane("%8", "@8", 0, Role.AGENT, 80, 24, "claude", "/secretary", false, false)),
                        )
                    }
                    emit(Message.SessionStarted(req.id, "$8", "ask-${req.workspace}"))
                    emit(Message.Result(req.id))
                }
                else -> super.answer(req)
            }
        }
    }

    private suspend fun connected(bridge: FakeBridge): PaneSession {
        val session = PaneSession(oneThread())
        session.connect {
            bridge.start()
            bridge
        }
        return session
    }

    @Test
    fun talkToAssistantOpensTheOverallWorkspacesRootSession() = runBlocking {
        val bridge = RootBridge(15)
        val session = connected(bridge)
        assertEquals("secretary", session.state.value.assistantWorkspace)
        val started = session.talkToAssistant()
        assertEquals("ask-secretary", started.name)
        assertTrue(session.state.value.sessions.any { it.id == "$8" })
        assertEquals("the open session is the answer again", started.session, session.talkToAssistant().session)
        val sent = bridge.received.filterIsInstance<Request.StartSession>()
        assertEquals(listOf("secretary" to true, "secretary" to true), sent.map { it.workspace to it.root })
        assertTrue("no repo with root", sent.all { it.repo == null })
    }

    @Test
    fun aLevel14HostOffersNoTalk() = runBlocking {
        val bridge = RootBridge(14)
        val session = connected(bridge)
        assertEquals("secretary", session.state.value.assistant?.overall)
        assertNull(session.state.value.assistantWorkspace)
        try {
            session.talkToAssistant()
            fail("a level 14 host cannot")
        } catch (e: PaneSession.Failure.Bridge) {
            assertEquals(PaneSession.ASSISTANT_UNSUPPORTED, PaneSession.describe(e))
        }
        assertTrue(bridge.received.none { it is Request.StartSession })
    }
}
