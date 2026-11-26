package tech.xdge.muxa.v2

import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class KeyBytesTest {
    private fun check(name: String, expected: String) = assertArrayEquals(name, expected.toByteArray(), KeyBytes.bytes(name))

    private fun check(name: String, vararg expected: Int) =
        assertArrayEquals(name, ByteArray(expected.size) { expected[it].toByte() }, KeyBytes.bytes(name))

    @Test
    fun aMessageKeepsItsReturnApart() {
        val long = "x".repeat(300)
        assertEquals(long to "\r", KeyBytes.message(long))
        assertEquals("\u001b[200~Fix it.\nAnd add a test.\u001b[201~" to "\r", KeyBytes.message("Fix it.\nAnd add a test."))
    }

    @Test
    fun namedKeysAreXtermSequences() {
        check("Up", "\u001b[A")
        check("S-Up", "\u001b[1;2A")
        check("C-M-Left", "\u001b[1;7D")
        check("PageDown", "\u001b[6~")
        check("F1", "\u001bOP")
        check("F5", "\u001b[15~")
        check("Enter", 13)
        check("BSpace", 0x7f)
        check("BTab", "\u001b[Z")
        check("Escape", 0x1b)
    }

    @Test
    fun controlAndMetaCharacters() {
        check("C-c", 3)
        check("C-Space", 0)
        check("C-[", 0x1b)
        check("M-x", 0x1b, 0x78)
        check("M-BSpace", 0x1b, 0x7f)
        check("M-Enter", 0x1b, 13)
        // Shift is not Meta.
        check("S-Enter", 13)
        check("S-BSpace", 0x7f)
        assertNull(KeyBytes.bytes("C-1"))
        assertNull(KeyBytes.bytes("Nonsense"))
    }
}
