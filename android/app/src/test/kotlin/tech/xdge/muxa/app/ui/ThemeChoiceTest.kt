package tech.xdge.muxa.app.ui

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import tech.xdge.muxa.session.Rgb
import tech.xdge.muxa.session.TerminalImport
import tech.xdge.muxa.session.ThemeAppearance
import tech.xdge.muxa.session.ThemePick
import tech.xdge.muxa.session.ThemeRegistry
import tech.xdge.muxa.session.ThemeToken

class ThemeChoiceTest {
    @get:Rule
    val dir = TemporaryFolder()

    private class Memory(var value: String? = null) : PickStorage {
        override fun read(): String? = value
        override fun write(value: String) {
            this.value = value
        }
    }

    private val bundled = ThemeRegistry.bundledFiles()

    private fun choice(storage: PickStorage, system: ThemeAppearance = ThemeAppearance.DARK) =
        ThemeChoice(bundled, dir.root.resolve("themes"), storage, system)

    @Test
    fun aNewDeviceHasMuxaDark() {
        assertEquals("muxa-dark", choice(Memory()).resolve().id)
    }

    @Test
    fun thePickIsKeptAsSwiftWritesIt() {
        val storage = Memory()
        assertEquals("dracula", choice(storage).choose("dracula").id)
        assertEquals("""{"fixed":"dracula"}""", storage.value)
        val again = choice(storage)
        assertEquals(ThemePick.Fixed("dracula"), again.pick)
        assertEquals("dracula", again.resolve().id)
    }

    @Test
    fun aPickThatIsNotJsonOrIsGoneShowsTheDefault() {
        assertEquals("muxa-dark", choice(Memory("not json")).resolve().id)
        assertEquals("muxa-dark", choice(Memory("""{"fixed":"gone"}""")).resolve().id)
    }

    @Test
    fun automaticFollowsTheSystemWhileTheAppRuns() {
        val storage = Memory()
        val choice = choice(storage, ThemeAppearance.LIGHT)
        assertEquals("muxa-light", choice.choose(ThemeChoice.AUTOMATIC).id)
        assertEquals("""{"system":{"light":"muxa-light","dark":"muxa-dark"}}""", storage.value)
        assertEquals("muxa-dark", choice.follow(ThemeAppearance.DARK).id)
        // A fixed pick does not change with the system.
        assertEquals("dracula", choice.choose("dracula").id)
        assertEquals("dracula", choice.follow(ThemeAppearance.LIGHT).id)
    }

    @Test
    fun anImportIsSavedAndPickedByItsKind() {
        val storage = Memory()
        val choice = choice(storage)
        val ghostty = choice.import("background = #101010\nforeground = #e0e0e0".toByteArray(), "Mine")
        assertEquals("Mine", ghostty.name)
        assertEquals(Rgb(0x10, 0x10, 0x10), ghostty.terminal.background)
        val json = """{"schema":1,"id":"paper","name":"Paper","extends":"muxa-light"}"""
        assertEquals("Paper", choice.import(json.toByteArray(), "paper.JSON").name)
        // Kept on the device: a new start lists both and keeps the pick.
        val again = choice(storage)
        assertTrue(again.registry.file(ghostty.id) != null)
        assertEquals("paper", again.resolve().id)
    }

    @Test
    fun anImportWithABuiltInsIdIsRefused() {
        val choice = choice(Memory())
        val json = """{"schema":1,"id":"dracula","name":"Mine","extends":"muxa-dark"}"""
        val error = assertThrows(TerminalImport.ImportError::class.java) { choice.import(json.toByteArray(), "x.json") }
        assertTrue(error.message.orEmpty().contains("built-in"))
        assertEquals("Dracula", choice.registry.resolve("dracula").name)
    }

    @Test
    fun removingThePickedThemeGoesBackToTheDefault() {
        val choice = choice(Memory())
        val edit = choice.registry.editable("dracula").setting(ThemeToken.accent, Rgb(0xFF, 0x79, 0xC6))
        assertEquals("dracula-custom", choice.save(edit).id)
        assertEquals(Rgb(0xFF, 0x79, 0xC6), choice.resolve()[ThemeToken.accent])
        assertEquals("muxa-dark", choice.remove("dracula-custom").id)
        assertFalse(choice.registry.files.any { it.id == "dracula-custom" })
        // A built-in is never removed.
        assertEquals("muxa-dark", choice.remove("dracula").id)
        assertTrue(choice.registry.file("dracula") != null)
    }
}
