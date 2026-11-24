package tech.xdge.muxa.session

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder

/** ADR 0091 Amendment 1: imports give Swift's palette, and edits go into a theme that extends the picked one. */
class TerminalImportTest {
    @get:Rule
    val dir = TemporaryFolder()

    private fun fixture(path: String): ByteArray =
        requireNotNull(javaClass.classLoader?.getResource("import/$path")) { "no fixture import/$path" }.readBytes()

    @Test
    fun importedFilesGiveTheSharedPalette() {
        val expected = Json.parseToJsonElement(fixture("expected.json").decodeToString()).jsonObject
        val terminal = expected.getValue("terminal").jsonObject
        fun hex(key: String) = terminal.getValue(key).jsonPrimitive.content
        val files = expected.getValue("files").jsonArray.map { it.jsonObject }
        assertEquals(2, files.size)
        files.forEach { f ->
            val path = f.getValue("file").jsonPrimitive.content
            val name = f.getValue("name").jsonPrimitive.content
            val bytes = fixture(path)
            val theme = if (f.getValue("kind").jsonPrimitive.content == "iterm") {
                TerminalImport.iTerm(bytes, name)
            } else {
                TerminalImport.ghostty(bytes.decodeToString(), name)
            }
            val t = requireNotNull(theme.terminal)
            assertEquals(path, name, theme.name)
            assertTrue(path, theme.id.startsWith("import-"))
            assertEquals(path, hex("background"), t.background)
            assertEquals(path, hex("foreground"), t.foreground)
            assertEquals(path, hex("cursor"), t.cursor)
            assertEquals(path, hex("selection"), t.selection)
            assertEquals(path, terminal.getValue("ansi").jsonArray.map { it.jsonPrimitive.content }, t.ansi)
        }
    }

    @Test
    fun eachImportHasItsOwnId() {
        val text = "background = #101010\nforeground = #e0e0e0"
        assertNotEquals(TerminalImport.ghostty(text, "a").id, TerminalImport.ghostty(text, "a").id)
    }

    @Test
    fun aPartPaletteIsDroppedAndMissingColorsAreRefused() {
        val partial = TerminalImport.ghostty("background = #101010\nforeground = e0e0e0\npalette = 0=#000000", "p")
        assertNull(partial.terminal?.ansi)
        val ghostty = assertThrows(TerminalImport.ImportError::class.java) { TerminalImport.ghostty("foreground = #ffffff", "x") }
        assertEquals("The theme has no background color.", ghostty.message)
        val notPlist = assertThrows(TerminalImport.ImportError::class.java) { TerminalImport.iTerm("not xml".toByteArray(), "x") }
        assertEquals("The file is not an iTerm2 color preset (.itermcolors).", notPlist.message)
    }

    @Test
    fun anExternalEntityIsNotRead() {
        val secret = dir.newFile("secret.txt").apply { writeText("0.5") }
        val plist = """<?xml version="1.0"?>
            <!DOCTYPE plist [<!ENTITY x SYSTEM "${secret.toURI()}">]>
            <plist><dict>
            <key>Background Color</key><dict><key>Red Component</key><real>&x;</real>
            <key>Green Component</key><real>0</real><key>Blue Component</key><real>0</real></dict>
            </dict></plist>"""
        val error = assertThrows(TerminalImport.ImportError::class.java) { TerminalImport.iTerm(plist.toByteArray(), "x") }
        assertEquals("The theme has no background color.", error.message)
    }

    @Test
    fun anEditGoesIntoAThemeThatExtendsThePickedOne() {
        val registry = ThemeRegistry(ThemeRegistry.bundledFiles())
        val pink = Rgb(0xFF, 0x79, 0xC6)
        val edit = registry.editable("dracula").setting(ThemeToken.accent, pink)
        assertEquals("dracula-custom", edit.id)
        assertEquals("Dracula (custom)", edit.name)
        assertEquals("dracula", edit.extends)
        assertEquals(mapOf("accent" to "#ff79c6"), edit.tokens)

        val saved = ThemeRegistry(ThemeRegistry.bundledFiles(), listOf(edit))
        assertEquals(pink, saved.resolve("dracula-custom")[ThemeToken.accent])
        assertEquals(saved.resolve("dracula")[ThemeToken.ground], saved.resolve("dracula-custom")[ThemeToken.ground])
        // The edited theme is edited again in place, and from its parent the same file is found.
        assertEquals("dracula-custom", saved.editable("dracula-custom").id)
        assertEquals("dracula-custom", saved.editable("dracula").id)
        // Back to the parent's color: no override is left.
        assertNull(edit.setting(ThemeToken.accent, null).tokens)
    }

    @Test
    fun aRemovedThemeIsGoneFromTheFolder() {
        val folder = dir.newFolder("themes")
        val file = TerminalImport.ghostty("background = #101010\nforeground = #e0e0e0", "Mine")
        ThemeRegistry.save(file, folder)
        assertEquals(listOf(file), ThemeRegistry.userFiles(folder).first)
        ThemeRegistry.remove(file.id, folder)
        assertTrue(ThemeRegistry.userFiles(folder).first.isEmpty())
        assertThrows(java.io.IOException::class.java) { ThemeRegistry.remove("../x", folder) }
    }
}
