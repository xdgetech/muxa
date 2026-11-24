package tech.xdge.muxa.app.ui

import org.junit.Assert.assertEquals
import org.junit.Test
import tech.xdge.muxa.session.ThemeAppearance
import tech.xdge.muxa.session.ThemePick
import tech.xdge.muxa.session.ThemeRegistry

class ThemeChoiceTest {
    private class Memory(var value: String? = null) : PickStorage {
        override fun read(): String? = value
        override fun write(value: String) {
            this.value = value
        }
    }

    private val registry = ThemeRegistry(ThemeRegistry.bundledFiles())

    private fun choice(storage: PickStorage) = ThemeChoice(registry, storage, ThemeAppearance.DARK)

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
}
