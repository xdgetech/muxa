package tech.xdge.muxa.session

import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import tech.xdge.muxa.protocol.Agent
import tech.xdge.muxa.protocol.AgentState
import tech.xdge.muxa.protocol.Bytes
import tech.xdge.muxa.protocol.ChangeInfo
import tech.xdge.muxa.protocol.ChatEntry
import tech.xdge.muxa.protocol.ChatPrompt
import tech.xdge.muxa.protocol.DiffScope
import tech.xdge.muxa.protocol.DocInfo
import tech.xdge.muxa.protocol.ImageInfo
import tech.xdge.muxa.protocol.Message
import tech.xdge.muxa.protocol.Request
import tech.xdge.muxa.protocol.Wire

/** The Reader's documents, diffs and images, and the chat log, through [PaneSession]. */
class ReaderSessionTest {
    private suspend fun connected(bridge: FakeBridge): PaneSession {
        val session = PaneSession(oneThread())
        session.connect {
            bridge.start()
            bridge
        }
        return session
    }

    private fun docIds(b: FakeBridge) = b.received.filterIsInstance<Request.Doc>().map { it.id }
    private fun diffIds(b: FakeBridge) = b.received.filterIsInstance<Request.Diff>().map { it.id }

    @Test
    fun listsThenOpensAndFollowsADocument() = runBlocking {
        val bridge = FakeBridge().apply {
            capabilities = listOf("docs")
            docs = listOf(DocInfo("plans/p.md", DocInfo.Kind.PLAN, "Plan"), DocInfo("README.md", DocInfo.Kind.REPO))
        }
        val session = connected(bridge)
        val listing = session.documents("%1")
        assertEquals("app", listing.root)
        assertEquals(listOf("plans/p.md", "README.md"), listing.entries.map { it.path })

        session.openDocument("%1", "README.md")
        val first = docIds(bridge).last()
        bridge.emit(Message.DocContent(first, "README.md", "# One", 1))
        waitUntil { session.state.value.doc?.text == "# One" }
        bridge.emit(Message.DocContent(first, "README.md", "# Two", 2))
        waitUntil { session.state.value.doc?.text == "# Two" }

        // Content for an earlier document is dropped once another is open.
        session.openDocument("%1", "plans/p.md")
        val second = docIds(bridge).last()
        bridge.emit(Message.DocContent(first, "README.md", "stale", 3))
        bridge.emit(Message.DocContent(second, "plans/p.md", "# Plan", 3))
        waitUntil { session.state.value.doc?.text == "# Plan" }
        bridge.emit(Message.DocEnded(second, "the file was removed"))
        waitUntil { session.state.value.doc?.ended == "the file was removed" }
    }

    @Test
    fun aReconnectOpensTheSameDocumentAgain() = runBlocking {
        val session = connected(FakeBridge().apply { capabilities = listOf("docs") })
        session.openDocument("%1", "README.md")
        val next = FakeBridge().apply { capabilities = listOf("docs") }
        session.connect {
            next.start()
            next
        }
        assertEquals(1, docIds(next).size)
        assertEquals("README.md", session.state.value.doc?.path)
        assertNull(session.state.value.doc?.text)
    }

    @Test
    fun nothingIsRequestedWithoutTheCapability() = runBlocking {
        val bridge = FakeBridge()
        val session = connected(bridge)
        session.openDocument("%1", "README.md")
        assertNull(session.state.value.doc)
        assertTrue(docIds(bridge).isEmpty())
    }

    @Test
    fun aBranchDiffSaysItsScopeAndAnUncommittedOneSaysNothing() = runBlocking {
        val bridge = FakeBridge().apply { capabilities = listOf("changes", "branch-changes") }
        val session = connected(bridge)
        session.openDiff("%1", "a.rs", DiffScope.BRANCH)
        session.openDiff("%1", "a.rs")
        assertEquals(listOf(DiffScope.BRANCH, null), bridge.received.filterIsInstance<Request.Diff>().map { it.scope })
        assertEquals(DiffScope.UNCOMMITTED, session.state.value.diff?.scope)
    }

    @Test
    fun listsChangesThenFollowsADiff() = runBlocking {
        val bridge = FakeBridge().apply {
            capabilities = listOf("changes")
            changes = listOf(
                ChangeInfo("a.rs", ChangeInfo.Status.MODIFIED, added = 1, removed = 1),
                ChangeInfo("logo.png", ChangeInfo.Status.ADDED),
            )
        }
        val session = connected(bridge)
        assertEquals(listOf("a.rs", "logo.png"), session.changes("%1").entries.map { it.path })

        session.openDiff("%1", "a.rs")
        val first = diffIds(bridge).last()
        assertFalse(session.state.value.diff!!.loaded)
        bridge.emit(Message.DiffContent(first, "a.rs", "@@ -1 +1 @@\n-a\n+b\n"))
        waitUntil { session.state.value.diff?.text?.contains("+b") == true }

        // A diff for an earlier request is dropped once another is open.
        session.openDiff("%1", "logo.png")
        val second = diffIds(bridge).last()
        bridge.emit(Message.DiffContent(first, "a.rs", "stale"))
        bridge.emit(Message.DiffContent(second, "logo.png", null, binary = true))
        waitUntil { session.state.value.diff?.binary == true }
        assertNull(session.state.value.diff?.text)
        bridge.emit(Message.DiffEnded(second, "logo.png has no changes now"))
        waitUntil { session.state.value.diff?.ended == "logo.png has no changes now" }

        session.closeDiff()
        assertNull(session.state.value.diff)
    }

    @Test
    fun listsThenOpensAndFollowsAnImage() = runBlocking {
        val bridge = FakeBridge().apply {
            capabilities = listOf("images")
            images = listOf(ImageInfo("shots/a.png", 2, 4))
        }
        val session = connected(bridge)
        assertEquals(listOf("shots/a.png"), session.images("%1").entries.map { it.path })

        session.openImage("%1", "shots/a.png")
        val id = bridge.received.filterIsInstance<Request.Image>().last().id
        bridge.emit(Message.ImageContent(id, "shots/a.png", Bytes(byteArrayOf(1, 2)), 2))
        waitUntil { session.state.value.image?.data?.contentEquals(byteArrayOf(1, 2)) == true }
        bridge.emit(Message.ImageContent(id + 100, "shots/a.png", Bytes(byteArrayOf(9)), 3))
        bridge.emit(Message.ImageEnded(id, "removed"))
        waitUntil { session.state.value.image?.ended == "removed" }
        assertArrayEquals(byteArrayOf(1, 2), session.state.value.image?.data)
    }

    // The chat log on its own.

    private fun entry(id: String, text: String = id) = ChatEntry(id, ChatEntry.Kind.USER, text)

    @Test
    fun resetsUpsertsAndDropsWhatWasAlreadyCovered() {
        var log = ChatLog("%1").apply(
            Message.ChatEntries(
                1, 1, reset = true, entries = listOf(entry("a"), entry("b")),
                status = Agent(AgentState.RUNNING), agent = "claude-code", truncated = true,
            ),
        )
        assertEquals(listOf("a", "b"), log.entries.map { it.id })
        assertEquals("claude-code", log.agent)
        assertTrue(log.truncated)
        assertTrue(log.loaded)

        log = log.apply(Message.ChatEntries(1, 2, entries = listOf(entry("b", "B"), entry("c"))))
        assertEquals(listOf("a", "B", "c"), log.entries.map { it.text })
        assertNull(log.status)

        // A late duplicate of an earlier message changes nothing.
        log = log.apply(Message.ChatEntries(1, 2, entries = listOf(entry("d"))))
        assertEquals(listOf("a", "b", "c"), log.entries.map { it.id })

        log = log.apply(Message.ChatEntries(1, 3, reset = true, entries = listOf(entry("z"))))
        assertEquals(listOf("z"), log.entries.map { it.id })
        assertFalse(log.truncated)
    }

    @Test
    fun thePromptIsTheLatestUpdates() {
        val prompt = ChatPrompt("t:1", ChatPrompt.Kind.PERMISSION, options = listOf(ChatPrompt.Option("Allow", listOf("1"))))
        var log = ChatLog("%1").apply(
            Message.ChatEntries(1, 1, reset = true, entries = emptyList(), status = Agent(AgentState.WAITING), prompt = prompt),
        )
        assertEquals(prompt, log.prompt)
        log = log.apply(Message.ChatEntries(1, 2, entries = emptyList(), status = Agent(AgentState.RUNNING)))
        assertNull(log.prompt)

        // A kind this client does not know still decodes, so the update is kept.
        val line = """{"type":"chat-entries","chat":1,"seq":3,"entries":[],""" +
            """"prompt":{"entry":"t:2","kind":"sandbox","options":[{"label":"Go","keys":["1"],"cancel":true}]}}"""
        val decoded = (Wire.decodeMessage(line) as Message.ChatEntries).prompt!!
        assertEquals(ChatPrompt.Kind("sandbox"), decoded.kind)
        assertTrue(decoded.options.first().cancel)
    }
}
