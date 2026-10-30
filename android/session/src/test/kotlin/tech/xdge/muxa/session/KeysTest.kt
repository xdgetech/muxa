package tech.xdge.muxa.session

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import tech.xdge.muxa.session.KeyMap.Modifiers
import tech.xdge.muxa.session.KeyMap.Special

class KeysTest {
    private fun keys(vararg k: String) = KeyOutput.Keys(k.toList())

    @Test
    fun specialKeysAreNamed() {
        val cases = listOf(
            Triple(Special.Up, Modifiers.NONE, "Up"),
            Triple(Special.Enter, Modifiers.NONE, "Enter"),
            Triple(Special.Enter, Modifiers(shift = true), "S-Enter"),
            Triple(Special.Tab, Modifiers(shift = true), "BTab"),
            Triple(Special.Backspace, Modifiers.NONE, "BSpace"),
            Triple(Special.Left, Modifiers(control = true, alt = true), "C-M-Left"),
            Triple(Special.Function(5), Modifiers.NONE, "F5"),
        )
        for ((key, mods, name) in cases) assertEquals(keys(name), KeyMap.map(key, "", mods))
    }

    @Test
    fun chordsUseHostKeyNames() {
        assertEquals(keys("C-c"), KeyMap.map(null, "c", Modifiers.CONTROL))
        assertEquals(keys("M-b"), KeyMap.map(null, "b", Modifiers(alt = true)))
        assertEquals(keys("C-A"), KeyMap.map(null, "A", Modifiers(control = true, shift = true)))
        assertEquals(keys("C-Space"), KeyMap.map(null, " ", Modifiers.CONTROL))
    }

    @Test
    fun plainCharactersAreText() {
        assertEquals(KeyOutput.Text("a"), KeyMap.map(null, "a", Modifiers.NONE))
        assertEquals(KeyOutput.Text("é"), KeyMap.map(null, "é", Modifiers(shift = true)))
    }

    @Test
    fun shortcutsAndUnnamedKeysStayInTheApp() {
        assertEquals(KeyOutput.None, KeyMap.map(null, "k", Modifiers(control = true, shift = true, shortcut = true)))
        assertEquals(KeyOutput.None, KeyMap.map(Special.Function(13), "", Modifiers.NONE))
        assertEquals(KeyOutput.None, KeyMap.map(null, "é", Modifiers.CONTROL))
    }

    @Test
    fun barKeysAreNamed() {
        val cases = listOf(
            KeyBar.Key.ESCAPE to "Escape", KeyBar.Key.TAB to "Tab", KeyBar.Key.BACK_TAB to "BTab",
            KeyBar.Key.INTERRUPT to "C-c", KeyBar.Key.LEFT to "Left", KeyBar.Key.DOWN to "Down",
            KeyBar.Key.UP to "Up", KeyBar.Key.RIGHT to "Right",
        )
        for ((key, name) in cases) assertEquals(keys(name), KeyBar().tap(key).second)
    }

    @Test
    fun barControlAppliesToTheNextCharacterOrKeyOnly() {
        val (pending, none) = KeyBar().tap(KeyBar.Key.CONTROL)
        assertEquals(KeyOutput.None, none)
        assertTrue(pending.control)
        val (released, chord) = pending.type(KeyOutput.Text("c"))
        assertEquals(keys("C-c"), chord)
        assertFalse(released.control)
        assertEquals(KeyOutput.Text("c"), released.type(KeyOutput.Text("c")).second)
        assertEquals(keys("C-Left"), pending.tap(KeyBar.Key.LEFT).second)
        assertFalse(pending.tap(KeyBar.Key.CONTROL).first.control)
    }

    @Test
    fun barControlSendsTextItCannotNameAsTypedAndWaitsThroughCompositionEdits() {
        val pending = KeyBar(control = true)
        val (released, text) = pending.type(KeyOutput.Text("日本"))
        assertEquals(KeyOutput.Text("日本"), text)
        assertFalse(released.control)
        assertTrue(pending.type(KeyOutput.None).first.control)
        assertEquals(keys("Enter"), pending.type(keys("Enter")).second)
    }

    @Test
    fun markedTextIsSentOnlyOnCommit() {
        val c = Composition().mark("に", 1, 1).mark("にほんご", 4, 4)
        assertTrue(c.isComposing)
        val (after, sent) = c.commit()
        assertEquals(KeyOutput.Text("にほんご"), sent)
        assertFalse(after.isComposing)
        assertEquals(KeyOutput.None, after.commit().second)
    }

    @Test
    fun convertedTextReplacesTheCompositionAndBackspaceEditsItFirst() {
        assertEquals(KeyOutput.Text("日本語"), Composition().mark("にほんご", 0, 4).insert("日本語").second)
        val (one, nothing) = Composition().mark("にほ", 2, 2).deleteBackward()
        assertEquals(KeyOutput.None, nothing)
        assertEquals("に", one.marked)
        val (empty, _) = one.deleteBackward()
        assertEquals(keys("BSpace"), empty.deleteBackward().second)
        assertEquals(keys("Enter"), Composition().insert("\n").second)
        assertEquals(2, Composition().mark("ab", 1, 9).selectionEnd)
    }
}
