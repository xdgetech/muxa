package tech.xdge.muxa.terminal

import org.junit.Assert.assertEquals
import org.junit.Test

class HistoryTest {
    private fun text(line: List<Run>) = line.joinToString("") { it.text }.trimEnd()

    @Test
    fun everyLineIsKeptWithItsColors() {
        val data = "\u001b[31mred\u001b[0m\nplain\nlast\n".toByteArray()
        val lines = History.lines(10, data)
        assertEquals(listOf("red", "plain", "last"), lines.map(::text))
        assertEquals(TermColor.Indexed(1), lines[0][0].style.foreground)
    }

    @Test
    fun aLineAsWideAsThePaneDoesNotAddAnEmptyLine() {
        val lines = History.lines(4, "abcd\nef\n".toByteArray())
        assertEquals(listOf("abcd", "ef"), lines.map(::text))
    }

    @Test
    fun aLongHistoryKeepsItsFirstLine() {
        val data = (1..3000).joinToString("") { "line $it\n" }.toByteArray()
        val lines = History.lines(20, data)
        assertEquals(3000, lines.size)
        assertEquals("line 1", text(lines.first()))
        assertEquals("line 3000", text(lines.last()))
    }

    @Test
    fun noNewlineIsNoLine() {
        assertEquals(emptyList<List<Run>>(), History.lines(10, ByteArray(0)))
    }
}
