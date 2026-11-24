package tech.xdge.muxa.session

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put
import kotlinx.serialization.json.putJsonObject
import java.io.File
import java.io.IOException

/** A terminal's colors; [ansi] null keeps the terminal view's own. */
data class TerminalPalette(
    val background: Rgb,
    val foreground: Rgb,
    val cursor: Rgb? = null,
    val selection: Rgb? = null,
    val ansi: List<Rgb>? = null,
)

/** A theme with every token set: what the app draws with. */
class ResolvedTheme internal constructor(
    val id: String,
    val name: String,
    val appearance: ThemeAppearance,
    val terminal: TerminalPalette,
    private val colors: Map<ThemeToken, Rgb>,
) {
    operator fun get(token: ThemeToken): Rgb = colors[token] ?: terminal.foreground
}

/**
 * The themes a device can pick, as Swift's `ThemeRegistry`: the bundled files
 * first, then the user's. The first file with an id wins, so a user file
 * cannot replace a built-in.
 */
class ThemeRegistry(bundled: List<ThemeFile>, user: List<ThemeFile> = emptyList()) {
    val files: List<ThemeFile>
    val bundledIds: Set<String> = bundled.map { it.id }.toSet()
    private val byId: Map<String, ThemeFile>

    init {
        val map = LinkedHashMap<String, ThemeFile>()
        (bundled + user).forEach { map.putIfAbsent(it.id, it) }
        byId = map
        files = map.values.toList()
    }

    fun file(id: String): ThemeFile? = byId[ALIASES[id] ?: id]

    /** A user file that did not load, with why. */
    data class Failure(val file: File, val error: Exception)

    /** Always gives a theme: an unknown id resolves as the default. */
    fun resolve(id: String): ResolvedTheme {
        val file = file(id) ?: file(DEFAULT_ID) ?: return derive(listOf(MUXA))
        return derive(chain(file))
    }

    fun resolve(pick: ThemePick, system: ThemeAppearance): ResolvedTheme = resolve(pick.id(system))

    /**
     * The file and its parents, the root first. A cycle or an unknown parent
     * ends the chain; a chain with no terminal colors starts from the default.
     */
    internal fun chain(file: ThemeFile): List<ThemeFile> {
        val links = ArrayDeque(listOf(file))
        val seen = mutableSetOf(file.id)
        while (true) {
            val parent = links.first().extends?.let(::file) ?: break
            if (!seen.add(parent.id)) break
            links.addFirst(parent)
        }
        if (links.any { it.terminal != null }) return links
        val root = file(DEFAULT_ID)
        if (root == null || root.id in seen) return listOf(MUXA) + links
        return chain(root) + links
    }

    /**
     * The user theme an edit of [id] goes into (ADR 0091 Amendment 1): a user theme
     * that extends another is edited in place; any other theme gets `<id>-custom`,
     * which extends it.
     */
    fun editable(id: String): ThemeFile {
        val base = file(id) ?: file(DEFAULT_ID) ?: MUXA
        if (base.id !in bundledIds && base.extends != null) return base
        val customId = "${base.id}-custom"
        file(customId)?.takeIf { customId !in bundledIds }?.let { return it }
        return ThemeFile(schema = ThemeFile.SCHEMA_VERSION, id = customId, name = "${base.name} (custom)", extends = base.id)
    }

    companion object {
        const val DEFAULT_ID = "muxa-dark"

        /** ADR 0080's ids, still saved on Apple devices. */
        val ALIASES = mapOf("muxa" to "muxa-dark")

        /** The muxa terminal colors, when no file gives any. */
        private val MUXA_TERMINAL = ThemeFile.Terminal(background = "#0a0c0f", foreground = "#d5d9df")
        private val MUXA = ThemeFile(schema = ThemeFile.SCHEMA_VERSION, id = "muxa", name = "muxa", terminal = MUXA_TERMINAL)

        /**
         * ADR 0091 decision 3: the last terminal palette in the chain, then each
         * token in order, where an override replaces it before later tokens read it.
         */
        internal fun derive(chain: List<ThemeFile>): ResolvedTheme {
            val me = chain.last()
            val source = chain.lastOrNull { it.terminal != null }?.terminal ?: MUXA_TERMINAL
            val terminal = TerminalPalette(
                background = Rgb.parse(source.background) ?: Rgb(0x0A, 0x0C, 0x0F),
                foreground = Rgb.parse(source.foreground) ?: Rgb(0xD5, 0xD9, 0xDF),
                cursor = source.cursor?.let(Rgb::parse),
                selection = source.selection?.let(Rgb::parse),
                ansi = source.ansi?.mapNotNull(Rgb::parse)?.takeIf { it.size == 16 },
            )
            val palette = ThemePalette(terminal.background, terminal.foreground, terminal.ansi)
            val overrides = mutableMapOf<ThemeToken, Rgb>()
            chain.forEach { file ->
                file.tokens.orEmpty().forEach { (key, value) ->
                    val token = ThemeToken.of(key)
                    val color = Rgb.parse(value)
                    if (token != null && color != null) overrides[token] = color
                }
            }
            val colors = mutableMapOf<ThemeToken, Rgb>()
            ThemeToken.entries.forEach { token ->
                colors[token] = overrides[token] ?: token.derive(palette) { colors[it] ?: palette.foreground }
            }
            val stated = chain.asReversed().firstNotNullOfOrNull { ThemeAppearance.of(it.appearance) }
            val measured = if (terminal.background.luminance < terminal.foreground.luminance) {
                ThemeAppearance.DARK
            } else {
                ThemeAppearance.LIGHT
            }
            return ResolvedTheme(me.id, me.name, stated ?: measured, terminal, colors)
        }

        /** The themes bundled with the app, in `themes/index.json` order. */
        fun bundledFiles(): List<ThemeFile> {
            val ids = Json.decodeFromString<List<String>>(resource("themes/index.json"))
            return ids.map { ThemeFile.parse(resource("themes/$it.json")) }
        }

        private fun resource(path: String): String =
            ThemeRegistry::class.java.classLoader?.getResource(path)?.readText()
                ?: throw IOException("The app has no bundled $path")

        /** The user's theme files in [folder], by name; files that do not load come back in the failures. */
        fun userFiles(folder: File): Pair<List<ThemeFile>, List<Failure>> {
            if (!folder.exists()) return emptyList<ThemeFile>() to emptyList()
            val entries = folder.listFiles() ?: return emptyList<ThemeFile>() to
                listOf(Failure(folder, IOException("The theme folder could not be listed")))
            val files = mutableListOf<ThemeFile>()
            val failures = mutableListOf<Failure>()
            entries.filter { it.extension == "json" }.sortedBy { it.name }.forEach { file ->
                try {
                    files += ThemeFile.parse(file.readText())
                } catch (e: ThemeFile.LoadError) {
                    failures += Failure(file, e)
                } catch (e: IOException) {
                    failures += Failure(file, e)
                }
            }
            return files to failures
        }

        /** Deletes the user theme [id] from [folder]; a theme with no file there is no error. */
        fun remove(id: String, folder: File) {
            if (!ThemeFile.validId(id)) throw IOException("The theme id $id is not a file name")
            val target = File(folder, "$id.json")
            if (target.exists() && !target.delete()) throw IOException("The theme $id could not be removed")
        }

        /** Writes [file] as `<id>.json` in [folder], replacing one with that id. */
        fun save(file: ThemeFile, folder: File) {
            file.check()
            if (!folder.isDirectory && !folder.mkdirs()) throw IOException("The theme folder could not be made")
            val target = File(folder, "${file.id}.json")
            val temp = File(folder, ".${file.id}.json.tmp")
            temp.writeText(file.encoded())
            if (!temp.renameTo(target)) {
                temp.delete()
                throw IOException("The theme ${file.id} could not be saved")
            }
        }
    }
}

/** What the device picked (ADR 0091 decision 7): one theme, or one for each system appearance. */
sealed interface ThemePick {
    data class Fixed(val id: String) : ThemePick
    data class System(val light: String, val dark: String) : ThemePick

    fun id(system: ThemeAppearance): String = when (this) {
        is Fixed -> id
        is System -> if (system == ThemeAppearance.LIGHT) light else dark
    }

    /** `{"fixed": id}` or `{"system": {"light": id, "dark": id}}`, as Swift writes it. */
    fun encoded(): String = when (this) {
        is Fixed -> buildJsonObject { put("fixed", id) }
        is System -> buildJsonObject { putJsonObject("system") { put("light", light); put("dark", dark) } }
    }.toString()

    companion object {
        /** Null when [text] is not a pick. */
        fun parse(text: String): ThemePick? = try {
            val o: JsonObject = Json.parseToJsonElement(text).jsonObject
            val system = o["system"]?.jsonObject
            when {
                system != null -> System(
                    system.getValue("light").jsonPrimitive.content, system.getValue("dark").jsonPrimitive.content,
                )
                else -> o["fixed"]?.jsonPrimitive?.content?.let(::Fixed)
            }
        } catch (e: IllegalArgumentException) {
            null
        } catch (e: NoSuchElementException) {
            null
        }
    }
}
