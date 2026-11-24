package tech.xdge.muxa.session

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.double
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder

/** ADR 0091: theme files and their resolution, against Swift's fixture `themes.json`. */
class ThemeRegistryTest {
    @get:Rule
    val dir = TemporaryFolder()

    private val fixture: JsonObject = Json.parseToJsonElement(
        javaClass.classLoader!!.getResource("themes.json")!!.readText(),
    ).jsonObject

    private fun registry() = ThemeRegistry(
        ThemeRegistry.bundledFiles(),
        fixture.getValue("themes").jsonArray.map { ThemeFile.parse(it.toString()) },
    )

    private fun color(hex: String) = Rgb.parse(hex)!!

    @Test
    fun mixAndContrastMatchTheFixture() {
        fixture.getValue("mix").jsonArray.map { it.jsonArray }.forEach { row ->
            val a = color(row[0].jsonPrimitive.content)
            val b = color(row[1].jsonPrimitive.content)
            assertEquals("$row", row[3].jsonPrimitive.content, a.mixed(b, row[2].jsonPrimitive.int).hex)
        }
        fixture.getValue("contrast").jsonArray.map { it.jsonArray }.forEach { row ->
            val ratio = color(row[0].jsonPrimitive.content).contrast(color(row[1].jsonPrimitive.content))
            assertEquals("$row", row[2].jsonPrimitive.double, ratio, 0.01)
        }
    }

    @Test
    fun everyCaseResolvesToTheFixturesBytes() {
        val order = fixture.getValue("tokenOrder").jsonArray.map { it.jsonPrimitive.content }
        assertEquals(ThemeToken.entries.map { it.key }, order)
        val registry = registry()
        val cases = fixture.getValue("cases") as JsonArray
        assertEquals(10, cases.size)
        cases.map { it.jsonObject }.forEach { c ->
            val name = c.getValue("resolve").jsonPrimitive.content
            val theme = registry.resolve(name)
            assertEquals(name, c.getValue("id").jsonPrimitive.content, theme.id)
            assertEquals(name, c.getValue("name").jsonPrimitive.content, theme.name)
            assertEquals(name, c.getValue("appearance").jsonPrimitive.content, theme.appearance.name.lowercase())
            val tokens = c.getValue("tokens").jsonObject
            ThemeToken.entries.forEach { token ->
                assertEquals("$name $token", tokens.getValue(token.key).jsonPrimitive.content, theme[token].hex)
            }
        }
    }

    @Test
    fun muxaDarkPinsEveryToken() {
        val file = ThemeRegistry.bundledFiles().first { it.id == ThemeRegistry.DEFAULT_ID }
        val tokens = file.tokens.orEmpty()
        assertEquals(ThemeToken.entries.map { it.key }.toSet(), tokens.keys)
        val theme = ThemeRegistry(listOf(file)).resolve("muxa-dark")
        ThemeToken.entries.forEach { assertEquals("$it", color(tokens.getValue(it.key)), theme[it]) }
        assertNull(theme.terminal.ansi)
    }

    @Test
    fun builtInsKeepTheContrastLimits() {
        val registry = ThemeRegistry(ThemeRegistry.bundledFiles())
        assertEquals(listOf("muxa-dark", "ghostty", "solarized-dark", "dracula"), registry.files.map { it.id })
        registry.files.forEach { file ->
            val t = registry.resolve(file.id)
            val ground = t[ThemeToken.ground]
            listOf(ThemeToken.text, ThemeToken.textSecondary, ThemeToken.failure).forEach {
                assertTrue("${file.id} $it", t[it].contrast(ground) >= 4.5)
            }
            listOf(ThemeToken.textTertiary, ThemeToken.accent, ThemeToken.working, ThemeToken.needsYou, ThemeToken.decide)
                .forEach { assertTrue("${file.id} $it", t[it].contrast(ground) >= 3.0) }
        }
    }

    @Test
    fun anOverrideFlowsIntoTheTokensBuiltOnIt() {
        val registry = registry()
        val base = registry.resolve("dracula")
        val pink = registry.resolve("dracula-pink")
        assertEquals(color("#ff79c6"), pink[ThemeToken.accent])
        listOf(ThemeToken.done, ThemeToken.infoBack, ThemeToken.infoText, ThemeToken.readText, ThemeToken.readBack)
            .forEach { assertNotEquals("$it", base[it], pink[it]) }
        assertEquals(base[ThemeToken.working], pink[ThemeToken.working])
        assertEquals(base.terminal.background, pink.terminal.background)
    }

    @Test
    fun oldIdsAndUnknownIdsStillResolve() {
        val registry = registry()
        assertEquals("muxa-dark", registry.resolve("muxa").id)
        assertEquals("muxa-dark", registry.resolve("no-such-theme").id)
        assertEquals("dracula", registry.resolve(ThemePick.Fixed("dracula"), ThemeAppearance.LIGHT).id)
        assertEquals("paper", registry.resolve(ThemePick.System("paper", "dracula"), ThemeAppearance.LIGHT).id)
    }

    @Test
    fun aUserFileCannotReplaceABuiltIn() {
        val fake = ThemeFile(1, "dracula", "Not Dracula", terminal = ThemeFile.Terminal("#000000", "#ffffff"))
        assertEquals("Dracula", ThemeRegistry(ThemeRegistry.bundledFiles(), listOf(fake)).resolve("dracula").name)
    }

    @Test
    fun refusesFilesItCannotRead() {
        fun reason(json: String): ThemeFile.LoadError.Reason? =
            try {
                ThemeFile.parse(json)
                null
            } catch (e: ThemeFile.LoadError) {
                e.reason
            }
        val terminal = """"terminal": {"background": "#000000", "foreground": "#ffffff"}"""
        assertEquals(ThemeFile.LoadError.Reason.NOT_JSON, reason("not json"))
        assertEquals(ThemeFile.LoadError.Reason.NEWER_SCHEMA, reason("""{"schema": 2, "id": "a", "name": "A", $terminal}"""))
        assertEquals(ThemeFile.LoadError.Reason.BAD_ID, reason("""{"schema": 1, "id": "../a", "name": "A", $terminal}"""))
        assertEquals(ThemeFile.LoadError.Reason.NO_NAME, reason("""{"schema": 1, "id": "a", "name": " ", $terminal}"""))
        assertEquals(ThemeFile.LoadError.Reason.NO_TERMINAL, reason("""{"schema": 1, "id": "a", "name": "A"}"""))
        assertEquals(
            ThemeFile.LoadError.Reason.BAD_COLOR,
            reason("""{"schema": 1, "id": "a", "name": "A", $terminal, "tokens": {"accent": "blue"}}"""),
        )
        assertNull(reason("""{"schema": 1, "id": "a", "name": "A", "extends": "dracula", "tokens": {"glow": "x"}}"""))
    }

    @Test
    fun savesAndLoadsUserFiles() {
        val folder = dir.root.resolve("themes")
        assertEquals(emptyList<ThemeFile>() to emptyList<ThemeRegistry.Failure>(), ThemeRegistry.userFiles(folder))
        val file = ThemeFile(1, "mine", "Mine", terminal = ThemeFile.Terminal("#101010", "#e0e0e0"))
        ThemeRegistry.save(file, folder)
        folder.resolve("broken.json").writeText("{")

        val (files, failures) = ThemeRegistry.userFiles(folder)
        assertEquals(listOf(file), files)
        assertEquals(listOf("broken.json"), failures.map { it.file.name })
        val theme = ThemeRegistry(ThemeRegistry.bundledFiles(), files).resolve("mine")
        assertEquals("Mine", theme.name)
        assertEquals(color("#101010"), theme[ThemeToken.ground])
    }

    @Test
    fun aPickRoundTripsAsSwiftWritesIt() {
        listOf(ThemePick.Fixed("dracula"), ThemePick.System("paper", "muxa-dark")).forEach {
            assertEquals(it, ThemePick.parse(it.encoded()))
        }
        assertEquals(ThemePick.Fixed("ghostty"), ThemePick.parse("""{"fixed": "ghostty"}"""))
        assertNull(ThemePick.parse("[]"))
    }
}
