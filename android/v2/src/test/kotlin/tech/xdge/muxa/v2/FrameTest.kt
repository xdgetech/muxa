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
    }

    @Test
    fun itemsDecodeFromTheWire() {
        val wire = """{"inbox":{"revision":3,"items":[{"id":"i1","kind":"todo","title":"milk","later_until":200,"created":5,"source":"capture"}],"done":[]}}"""
        val inbox = field<Inbox>("inbox", json.parseToJsonElement(wire) as JsonObject)
        assertEquals(200L, inbox.items[0].laterUntil)
        assertTrue(inbox.items[0].isLater(100))
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
