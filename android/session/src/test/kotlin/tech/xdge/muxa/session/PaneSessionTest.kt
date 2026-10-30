package tech.xdge.muxa.session

import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test
import tech.xdge.muxa.protocol.Bytes
import tech.xdge.muxa.protocol.ChatEntry
import tech.xdge.muxa.protocol.ErrorBody
import tech.xdge.muxa.protocol.ErrorCode
import tech.xdge.muxa.protocol.Message
import tech.xdge.muxa.protocol.Request
import tech.xdge.muxa.terminal.PaneScreen

class PaneSessionTest {
    private suspend fun connected(bridge: FakeBridge = FakeBridge()): Triple<PaneSession, PaneScreen, FakeBridge> {
        val session = PaneSession(oneThread())
        val screen = PaneScreen()
        session.renderer = screen
        session.connect {
            bridge.start()
            bridge
        }
        return Triple(session, screen, bridge)
    }

    private fun PaneScreen.settledRows(): List<String> {
        awaitIdle()
        return rows()
    }

    @Test
    fun connectReadsHelloAndLists() = runBlocking {
        val (session, _, _) = connected()
        val s = session.state.value
        assertEquals(PaneSession.Phase.Connected, s.phase)
        assertEquals("3.7c", s.tmuxVersion)
        assertEquals(listOf("demo"), s.sessions.map { it.name })
    }

    @Test
    fun aBridgeThatExitsBeforeHelloSaysItDidNotStart() = runBlocking {
        val bridge = FakeBridge()
        val session = PaneSession(oneThread())
        try {
            session.connect {
                launch { bridge.close() }
                bridge
            }
            fail("connect should fail")
        } catch (_: PaneSession.Failure.NoHello) {
        }
        assertEquals(
            PaneSession.Phase.Disconnected(PaneSession.describe(PaneSession.Failure.NoHello())),
            session.state.value.phase,
        )
    }

    @Test
    fun aProtocolMismatchFailsTheConnect() = runBlocking {
        val bridge = FakeBridge().apply { protocolVersion = 2 }
        val session = PaneSession(oneThread())
        try {
            session.connect {
                bridge.start()
                bridge
            }
            fail("connect should fail")
        } catch (e: PaneSession.Failure.ProtocolMismatch) {
            assertEquals(2, e.bridge)
        }
        assertTrue(session.state.value.phase is PaneSession.Phase.Disconnected)
        assertTrue(bridge.closed)
    }

    @Test
    fun watchRendersTheScreenAndDropsStaleWatches() = runBlocking {
        val (session, screen, bridge) = connected()
        session.watch("%1")
        waitUntil { screen.settledRows() == listOf("hello") }

        // Output for an older watch must not reach the renderer.
        bridge.emit(Message.Output(999, "%1", Bytes(" stale".toByteArray())))
        val watch = bridge.received.filterIsInstance<Request.Watch>().last().id
        bridge.emit(Message.Output(watch, "%1", Bytes(" world".toByteArray())))
        waitUntil { screen.settledRows() == listOf("hello world") }
    }

    @Test
    fun inputKeepsItsOrder() = runBlocking {
        val (session, _, bridge) = connected()
        session.watch("%1")
        session.send("a")
        session.send(listOf("Enter"))
        session.send(byteArrayOf(0x1b))
        session.paste("p")
        waitUntil { bridge.received.size == 6 }
        val tail = bridge.received.takeLast(4).map {
            when (it) {
                is Request.Text -> "text ${it.text}"
                is Request.Keys -> "keys ${it.keys.joinToString("")}"
                is Request.RawBytes -> "bytes ${it.data.size}"
                is Request.Paste -> "paste ${it.text}"
                else -> "other"
            }
        }
        assertEquals(listOf("text a", "keys Enter", "bytes 1", "paste p"), tail)
    }

    @Test
    fun paneGoneIsReported() = runBlocking {
        val (session, _, bridge) = connected()
        session.watch("%1")
        val watch = bridge.received.filterIsInstance<Request.Watch>().last().id
        bridge.emit(Message.PaneGone(watch, "%1"))
        waitUntil { session.state.value.paneGoneReason != null }
    }

    @Test
    fun byeFromTheBridgeDisconnects() = runBlocking {
        val (session, _, bridge) = connected()
        bridge.emit(Message.Bye("idle timeout"))
        waitUntil { session.state.value.phase == PaneSession.Phase.Disconnected("idle timeout") }
    }

    @Test
    fun disconnectSendsBye() = runBlocking {
        val (session, _, bridge) = connected()
        session.disconnect()
        waitUntil { bridge.received.lastOrNull() is Request.Bye }
        assertTrue(bridge.closed)
    }

    @Test
    fun reconnectWatchesTheSamePane() = runBlocking {
        val (session, screen, first) = connected()
        session.watch("%1")
        first.close()
        waitUntil { session.state.value.phase is PaneSession.Phase.Disconnected }

        val second = FakeBridge().apply { this.screen = "\u001bcagain".toByteArray() }
        session.connect {
            second.start()
            second
        }
        waitUntil { screen.settledRows() == listOf("again") }
        assertEquals("%1", session.state.value.watchedPane)
    }

    @Test
    fun aFailedInputIsShown() = runBlocking {
        val bridge = object : FakeBridge() {
            override fun answer(req: Request) {
                if (req is Request.Keys) {
                    emit(Message.Result(req.id, ErrorBody(ErrorCode.UNSUPPORTED_KEY, "unsupported key ${req.keys[0]}")))
                } else {
                    super.answer(req)
                }
            }
        }
        val (session, _, _) = connected(bridge)
        session.watch("%1")
        session.send(listOf("Nope"))
        waitUntil { session.state.value.lastError == "unsupported key Nope" }
    }

    @Test
    fun aNewTaskReturnsItsSessionAfterListingAgain() = runBlocking {
        val bridge = FakeBridge().apply { capabilities = listOf("tasks") }
        val (session, _, _) = connected(bridge)
        assertTrue(session.state.value.canStartTasks)
        assertEquals("feat-x", session.newTask("/w/app", "feat/x", null))
        assertTrue(session.state.value.sessions.any { it.name == "feat-x" })
        assertTrue(bridge.received.any { it is Request.NewTask && it.repo == "/w/app" && it.branch == "feat/x" && it.base == null })
        try {
            session.newTask("/w/app", "-x", null)
            fail("a bad branch should fail")
        } catch (_: PaneSession.Failure.Bridge) {
        }
    }

    // Chat

    private fun entry(id: String, text: String = id) = ChatEntry(id, ChatEntry.Kind.USER, text)

    private fun chatIds(bridge: FakeBridge) = bridge.received.filterIsInstance<Request.Chat>().map { it.id }

    @Test
    fun followsAChatAndDropsAnEarlierOne() = runBlocking {
        val bridge = FakeBridge().apply { capabilities = listOf("chat") }
        val (session, _, _) = connected(bridge)
        session.follow("%1")
        val first = chatIds(bridge).last()
        bridge.emit(Message.ChatEntries(first, 1, reset = true, entries = listOf(entry("a"))))
        waitUntil { session.state.value.chat?.entries?.map { it.id } == listOf("a") }

        // After a new chat, the old one's messages are ignored.
        session.unfollow()
        session.follow("%1")
        val second = chatIds(bridge).last()
        bridge.emit(Message.ChatEntries(first, 2, entries = listOf(entry("stale"))))
        bridge.emit(Message.ChatEntries(second, 1, reset = true, entries = listOf(entry("new"))))
        waitUntil { session.state.value.chat?.entries?.map { it.id } == listOf("new") }

        bridge.emit(Message.ChatEnded(second, "Claude Code exited"))
        waitUntil { session.state.value.chat?.ended == "Claude Code exited" }
    }

    @Test
    fun anUnsupportedPaneIsRememberedAndNoChatWithoutTheCapability() = runBlocking {
        val bridge = FakeBridge().apply {
            capabilities = listOf("chat")
            unsupportedChats = setOf("%9")
        }
        val (session, _, _) = connected(bridge)
        session.follow("%9")
        assertNull(session.state.value.chat)
        assertEquals(setOf("%9"), session.state.value.unsupportedChats)
        session.follow("%9")
        assertEquals("an unsupported pane is not asked again", 1, chatIds(bridge).size)

        val old = FakeBridge()
        val (plain, _, _) = connected(old)
        plain.follow("%1")
        assertNull(plain.state.value.chat)
        assertTrue(chatIds(old).isEmpty())
    }

    @Test
    fun aReconnectFollowsTheSameChatAgain() = runBlocking {
        val bridge = FakeBridge().apply { capabilities = listOf("chat") }
        val (session, _, _) = connected(bridge)
        session.watch("%1")
        session.follow("%1")

        val next = FakeBridge().apply { capabilities = listOf("chat") }
        session.connect {
            next.start()
            next
        }
        assertEquals(1, chatIds(next).size)
        assertEquals("%1", session.state.value.chat?.pane)
    }
}
