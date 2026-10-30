package tech.xdge.muxa.terminal

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import tech.xdge.muxa.protocol.Message
import tech.xdge.muxa.protocol.Wire
import java.io.File

/**
 * Replays recorded bridge streams (`protocol/fixtures/scenarios`) into JediTerm.
 * The expected rows and cursor came from the host itself.
 */
class ScenarioTest {
    private val dir = File(System.getProperty("muxa.fixtures")).resolve("scenarios")

    @Test
    fun recordedStreamsRebuildTheExpectedScreen() {
        val files = dir.listFiles { f -> f.extension == "jsonl" }.orEmpty().sortedBy { it.name }
        assertFalse(files.isEmpty())
        for (file in files) {
            val lines = file.readLines().filter { it.isNotEmpty() }
            val header = Json.parseToJsonElement(lines[0]).jsonObject
            val name = header["scenario"]!!.jsonPrimitive.content
            val expectRows = header["expectRows"]!!.jsonArray.map { it.jsonPrimitive.content }
            val expectCursor = header["expectCursor"]!!.jsonArray.map { it.jsonPrimitive.int }

            val screen = PaneScreen()
            var watch: Long? = null
            for (line in lines.drop(1)) {
                when (val m = Wire.decodeMessage(line)) {
                    is Message.Screen -> {
                        watch = m.watch
                        screen.reset(m.cols, m.rows, m.data.array)
                    }
                    is Message.Output -> if (m.watch == watch) screen.feed(m.data.array)
                    else -> {}
                }
            }
            assertTrue(name, screen.awaitIdle())
            assertEquals(name, expectRows, screen.rows())
            val s = screen.snapshot()
            assertEquals(name, expectCursor, listOf(s.cursorRow, s.cursorColumn))
            screen.close()
        }
    }

    @Test
    fun aCharacterCutBetweenTwoFeedsStaysWhole() {
        val screen = PaneScreen()
        val bytes = "日本".toByteArray()
        screen.reset(10, 2, byteArrayOf(0x1b, 'c'.code.toByte()) + bytes.copyOfRange(0, 2))
        screen.feed(bytes.copyOfRange(2, bytes.size))
        assertTrue(screen.awaitIdle())
        assertEquals(listOf("日本"), screen.rows())
        screen.close()
    }
}
