package tech.xdge.muxa.v2

import kotlinx.serialization.json.JsonObject
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class FrameTest {
    private fun bytes(vararg b: Int) = ByteArray(b.size) { b[it].toByte() }

    @Test
    fun outputEncodesAsTheRustServerDoes() {
        // muxa_protocol::v2::Frame::output(3, 42, b"hi").encode()
        assertArrayEquals(
            bytes(3, 0, 0, 0, 0, 10, 0, 0, 0, 42, 0, 0, 0, 0, 0, 0, 0, 104, 105),
            Frame.withU64(3, 42, "hi".toByteArray()).encode(),
        )
        assertArrayEquals(bytes(1, 0, 0, 0, 3, 4, 0, 0, 0, 80, 0, 24, 0), Frame.resize(1, 80, 24).encode())
    }

    @Test
    fun framesComeOutOnlyWhenWhole() {
        val a = Frame.withU64(3, 42, "hi".toByteArray()).encode()
        val b = Frame.ack(3, 7).encode()
        val r = FrameReader()
        assertTrue(r.push(a.copyOfRange(0, 5)).isEmpty())
        val frames = r.push(a.copyOfRange(5, a.size) + b)
        assertEquals(42L, frames[0].splitU64()?.first)
        assertArrayEquals("hi".toByteArray(), frames[0].splitU64()?.second)
        assertEquals(FrameKind.ACK, frames[1].kind)
    }

    @Test(expected = FrameError.UnknownKind::class)
    fun anUnknownKindIsAnError() {
        FrameReader().push(bytes(0, 0, 0, 0, 9, 0, 0, 0, 0))
    }

    @Test
    fun requestsMatchTheSpec() {
        val add = V2Request.InboxAdd(Capture(ItemKind.REPLY, "bank", due = "2026-12-01")).json(4)
        assertEquals(
            """{"id":4,"type":"inbox-add","kind":"reply","title":"bank","due":"2026-12-01"}""",
            add.toString(),
        )
        val start = V2Request.StartTask("w", "Fix", connections = listOf("linear"), mcp = listOf("docs")).json(5)
        assertEquals("""["linear"]""", start["connections"].toString())
        assertEquals("""["docs"]""", start["mcp"].toString())
        // The deprecated names (ADR 0075) stay off the wire when none are given.
        val plain = V2Request.StartTask("w", "Fix").json(6)
        assertEquals(null, plain["connections"])
        assertEquals(null, plain["mcp"])
        val resume = V2Request.StartTask("app", "Tidy", places = listOf(PlaceSpec.Existing("/w/tasks/tidy"))).json(7)
        assertEquals("""[{"kind":"existing","path":"/w/tasks/tidy"}]""", resume["places"].toString())
        assertEquals("""{"id":8,"type":"inbox-delete-older","days":3}""", V2Request.InboxDeleteOlder(3).json(8).toString())
    }

    @Test
    fun aLoopHeldForALimitSaysWhenItGoesOn() {
        val wire = """{"loop":{"terminal":"t1","check":"make test","max_attempts":3,"timeout":600,"state":"paused","attempts":1,"armed_at":1,"since":2,"reason":"limit-reset","resumes_at":1700003600}}"""
        val lp = field<LoopInfo>("loop", json.parseToJsonElement(wire) as JsonObject)
        assertEquals("limit-reset", lp.reason)
        assertEquals(1_700_003_600L, lp.resumesAt)
        assertTrue(lp.isActive)
    }

    @Test
    fun itemsDecodeFromTheWire() {
        // An older server may still send `later_until`; it is ignored (ADR 0096).
        val wire = """{"inbox":{"revision":3,"items":[{"id":"i1","kind":"todo","title":"milk","later_until":200,"created":5,"source":"capture"}],"done":[]}}"""
        val inbox = field<Inbox>("inbox", json.parseToJsonElement(wire) as JsonObject)
        assertEquals("milk", inbox.items[0].title)
        assertTrue(inbox.items[0].links.isEmpty() && inbox.bots.isEmpty())
    }

    @Test
    fun laterSendsNoTime() {
        assertEquals("""{"id":4,"type":"inbox-later","item":"i1"}""", V2Request.InboxLater("i1").json(4).toString())
    }

    @Test
    fun aBotItemDecodesWithItsLinksAndProposal() {
        val wire = """{"inbox":{"revision":1,"items":[{"id":"i2","kind":"decide","title":"Comment on PR 7","created":5,""" +
            """"source":"routine","workspace":"w","bot":"pr-watch","links":[{"url":"https://github.com/o/r/pull/7","title":"PR 7"}],""" +
            """"proposal":{"action":"comment","target":"o/r#7","content":"LGTM"}}],"done":[],""" +
            """"bots":[{"workspace":"w","bot":"pr-watch","done":4,"deleted":1}]}}"""
        val inbox = field<Inbox>("inbox", json.parseToJsonElement(wire) as JsonObject)
        val item = inbox.items[0]
        assertEquals("pr-watch", item.bot)
        assertEquals(listOf(ItemLink("https://github.com/o/r/pull/7", "PR 7")), item.links)
        assertEquals(ItemProposal("comment", "o/r#7", "LGTM"), item.proposal)
        assertEquals(listOf(BotCount("w", "pr-watch", 4, 1)), inbox.bots)
    }

    @Test
    fun anItemFromV1HasItsPlace() {
        val wire = """{"inbox":{"revision":3,"items":[{"id":"i1","kind":"todo","title":"Resume: Tidy","created":5,"source":"v1",""" +
            """"place":{"workspace":"app","path":"/w/tasks/tidy"}},{"id":"i2","kind":"todo","title":"milk","created":5,"source":"capture"}],"done":[]}}"""
        val inbox = field<Inbox>("inbox", json.parseToJsonElement(wire) as JsonObject)
        assertEquals(ItemPlace("app", "/w/tasks/tidy"), inbox.items[0].place)
        assertEquals(null, inbox.items[1].place)
        // An item without a place sends none.
        assertEquals(null, (json.encodeToJsonElement(InboxItem.serializer(), inbox.items[1]) as JsonObject)["place"])
    }

    @Test
    fun readerEntriesSayWhenGitIgnoresThem() {
        val wire = """{"entries":[{"path":"README.md"},{"path":"output/qa/report.md","ignored":true}]}"""
        val entries = field<List<ReaderEntry>>("entries", json.parseToJsonElement(wire) as JsonObject)
        assertEquals(listOf(ReaderEntry("README.md"), ReaderEntry("output/qa/report.md", ignored = true)), entries)
        // A tracked file sends no `ignored`.
        assertEquals(null, (json.encodeToJsonElement(ReaderEntry.serializer(), entries[0]) as JsonObject)["ignored"])
    }

    @Test
    fun pausedTasksTravelAsTheSpecSays() {
        assertEquals("""{"id":1,"type":"pause-task","terminal":"t1"}""", V2Request.PauseTask("t1").json(1).toString())
        assertEquals("""{"id":2,"type":"resume-task","terminal":"t1"}""", V2Request.ResumeTask("t1").json(2).toString())
        assertEquals("""{"id":3,"type":"paused-tasks"}""", V2Request.PausedTasks.json(3).toString())
        val wire = """{"paused":[{"terminal":{"id":"t1","name":"fix","live":false,"status":null,
            "labels":{"muxa.kind":"task","muxa.role":"agent","muxa.title":"Fix it"},"cols":80,"rows":24},
            "paused_at":1764000000,"loop":{"check":"make test","max_attempts":3,"timeout":600}},
            {"terminal":{"id":"t2","live":false,"cols":80,"rows":24},"paused_at":1764000001}]}"""
        val paused = field<List<Paused>>("paused", json.parseToJsonElement(wire) as JsonObject)
        assertEquals("Fix it" to 1_764_000_000L, paused[0].terminal.taskTitle to paused[0].pausedAt)
        assertEquals(LoopSpec("make test", 3, 600), paused[0].loop)
        // No loop was active.
        assertEquals(null, paused[1].loop)
    }

    @Test
    fun connectionsAndServersSayWhoUsesThem() {
        val wire = """{"workspaces":[{"name":"a","root":"/a","repos":[],"loop":null,
            "connections":[{"name":"github","default":false,"used_by":{"routines":["digest"],"tasks":["t4"]},"last_used":1700000000}],
            "mcp":[{"name":"docs","default":true,"used_by":{"routines":[],"tasks":[]},"last_used":null}]},
            {"name":"old","root":"/o","repos":[],"connections":[{"name":"gh","default":true}]}]}"""
        val ws = field<List<Workspace>>("workspaces", json.parseToJsonElement(wire) as JsonObject)
        assertEquals(
            WorkspaceConnection("github", isDefault = false, usedBy = UsedBy(listOf("digest"), listOf("t4")), lastUsed = 1_700_000_000),
            ws[0].connections.single(),
        )
        assertEquals(WorkspaceMcp("docs", isDefault = true), ws[0].mcp.single())
        // A server before ADR 0069 §7 sends neither.
        assertEquals(WorkspaceConnection("gh", isDefault = true), ws[1].connections.single())
    }
}
