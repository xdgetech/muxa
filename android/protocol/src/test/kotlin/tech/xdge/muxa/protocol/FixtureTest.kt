package tech.xdge.muxa.protocol

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Test
import java.io.File

/**
 * `protocol/fixtures` is the contract: every line decodes and re-encodes to the same
 * JSON value. Key order may differ, so the trees are compared.
 */
class FixtureTest {
    private val fixtures = File(System.getProperty("muxa.fixtures"))

    private fun lines(name: String) = fixtures.resolve(name).readLines().filter { it.isNotEmpty() }

    private fun tree(line: String): JsonElement = sorted(Json.parseToJsonElement(line))

    private fun sorted(e: JsonElement): JsonElement = when (e) {
        is JsonObject -> JsonObject(e.toSortedMap().mapValues { sorted(it.value) })
        is JsonArray -> JsonArray(e.map(::sorted))
        else -> e
    }

    @Test
    fun requestsRoundTrip() {
        val lines = lines("requests.jsonl")
        assertEquals(23, lines.size)
        for (line in lines) {
            assertEquals(line, tree(line), tree(Wire.encode(Wire.decodeRequest(line))))
        }
    }

    @Test
    fun messagesRoundTrip() {
        val lines = lines("messages.jsonl")
        assertEquals(30, lines.size)
        for (line in lines) {
            val message = Wire.decodeMessage(line)
            assertFalse("fixture type is unknown: $line", message is Message.Unknown)
            assertEquals(line, tree(line), tree(Wire.encode(message)))
        }
    }

    @Test
    fun unknownTypesAndFieldsAreIgnored() {
        assertEquals(Message.Unknown("future-thing"), Wire.decodeMessage("""{"type":"future-thing","x":1}"""))
        assertEquals(
            Message.PaneGone(watch = 3, pane = "%1"),
            Wire.decodeMessage("""{"type":"pane-gone","watch":3,"pane":"%1","extra":true}"""),
        )
    }

    @Test
    fun newErrorCodesAndRolesStillDecode() {
        val result = Wire.decodeMessage("""{"type":"result","id":1,"error":{"code":"rate-limited","message":"m"}}""")
        assertEquals(ErrorCode("rate-limited"), (result as Message.Result).error?.code)
        val session = """{"type":"result","id":1,"sessions":[{"id":"$1","name":"s","panes":[{"id":"%1","window":"@1",""" +
            """"index":0,"role":"observer","cols":80,"rows":24,"command":"sh","path":"/","dead":false,"shellForeground":true}]}]}"""
        val pane = (Wire.decodeMessage(session) as Message.Result).sessions!!.single().panes.single()
        assertEquals(Role.UNKNOWN, pane.role)
    }

    @Test
    fun bytesAreBase64() {
        val line = Wire.encode(Request.RawBytes(1, "%1", Bytes(byteArrayOf(0x1b, 0xff.toByte()))))
        assertEquals(tree("""{"type":"bytes","id":1,"pane":"%1","data":"G/8="}"""), tree(line))
    }
}
