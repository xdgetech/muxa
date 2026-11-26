package tech.xdge.muxa.session

import org.w3c.dom.Element
import org.xml.sax.InputSource
import org.xml.sax.SAXException
import java.io.ByteArrayInputStream
import java.io.IOException
import java.io.StringReader
import java.util.UUID
import javax.xml.parsers.DocumentBuilderFactory
import javax.xml.parsers.ParserConfigurationException

/**
 * Reads iTerm2 and Ghostty themes into terminal-only theme files, with the rules of
 * Swift's `TerminalTheme.iTerm` and `.ghostty` (ADR 0091 Amendment 1), so both give
 * the same palette. Each import gets a fresh id.
 */
object TerminalImport {
    /** Why a file was refused; the message says what is wrong. */
    class ImportError(message: String) : Exception(message) {
        companion object {
            fun notAPlist() = ImportError("The file is not an iTerm2 color preset (.itermcolors).")
            fun missing(what: String) = ImportError("The theme has no $what.")
        }
    }

    /**
     * An iTerm2 color preset: a plist with `Ansi 0 Color` … `Ansi 15 Color`,
     * `Background Color` and `Foreground Color`, each with components from 0 to 1.
     */
    fun iTerm(bytes: ByteArray, name: String): ThemeFile {
        val plist = plistDict(bytes) ?: throw ImportError.notAPlist()
        fun color(key: String): Rgb? {
            val c = plist[key]?.let(::dict) ?: return null
            val r = c["Red Component"]?.let(::number) ?: return null
            val g = c["Green Component"]?.let(::number) ?: return null
            val b = c["Blue Component"]?.let(::number) ?: return null
            return Rgb(unitByte(r), unitByte(g), unitByte(b))
        }
        val background = color("Background Color") ?: throw ImportError.missing("background color")
        val foreground = color("Foreground Color") ?: throw ImportError.missing("foreground color")
        val ansi = (0 until 16).mapNotNull { color("Ansi $it Color") }
        return terminalOnly(name, background, foreground, color("Cursor Color"), color("Selection Color"), ansi)
    }

    /**
     * A Ghostty theme or config: `background = #282c34`, `foreground = …`,
     * `cursor-color = …`, `selection-background = …` and `palette = N=#rrggbb`.
     * Other keys and `#` comments are skipped.
     */
    fun ghostty(text: String, name: String): ThemeFile {
        val values = mutableMapOf<String, String>()
        val palette = mutableMapOf<Int, Rgb>()
        for (raw in text.lines()) {
            val line = raw.trim()
            val eq = line.indexOf('=')
            if (line.startsWith("#") || eq < 0) continue
            val key = line.substring(0, eq).trim()
            val value = line.substring(eq + 1).trim()
            val entry = if (key == "palette") paletteEntry(value) else null
            if (entry != null) palette[entry.first] = entry.second else values[key] = value
        }
        fun color(key: String): Rgb? = values[key]?.let(Rgb::parse)
        val background = color("background") ?: throw ImportError.missing("background color")
        val foreground = color("foreground") ?: throw ImportError.missing("foreground color")
        val ansi = (0 until 16).mapNotNull { palette[it] }
        return terminalOnly(name, background, foreground, color("cursor-color"), color("selection-background"), ansi)
    }

    private fun paletteEntry(value: String): Pair<Int, Rgb>? {
        val split = value.indexOf('=')
        if (split < 0) return null
        val n = value.substring(0, split).trim().toIntOrNull()?.takeIf { it in 0 until 16 } ?: return null
        val color = Rgb.parse(value.substring(split + 1).trim()) ?: return null
        return n to color
    }

    private fun terminalOnly(name: String, background: Rgb, foreground: Rgb, cursor: Rgb?, selection: Rgb?, ansi: List<Rgb>) =
        ThemeFile(
            schema = ThemeFile.SCHEMA_VERSION,
            id = "import-${UUID.randomUUID().toString().uppercase()}",
            name = name,
            terminal = ThemeFile.Terminal(
                background = background.hex,
                foreground = foreground.hex,
                cursor = cursor?.hex,
                selection = selection?.hex,
                ansi = ansi.takeIf { it.size == 16 }?.map { it.hex },
            ),
        )

    // Swift's `.rounded()` rounds half away from zero; Math.round rounds half up, the same for x ≥ 0.
    private fun unitByte(x: Double): Int = Math.round(x.coerceIn(0.0, 1.0) * 255).toInt()

    /** The plist's top dictionary, key → value element; null when the bytes are not one. */
    private fun plistDict(bytes: ByteArray): Map<String, Element>? {
        val factory = DocumentBuilderFactory.newInstance().apply {
            isExpandEntityReferences = false
            isNamespaceAware = false
            isValidating = false
        }
        val root = try {
            val builder = factory.newDocumentBuilder()
            // The file is untrusted: the plist DTD and any other external entity resolve to nothing.
            builder.setEntityResolver { _, _ -> InputSource(StringReader("")) }
            builder.setErrorHandler(null)
            builder.parse(ByteArrayInputStream(bytes)).documentElement
        } catch (e: SAXException) {
            return null
        } catch (e: IOException) {
            return null
        } catch (e: ParserConfigurationException) {
            return null
        }
        if (root?.tagName != "plist") return null
        return children(root).firstOrNull()?.let(::dict)
    }

    private fun dict(element: Element): Map<String, Element>? {
        if (element.tagName != "dict") return null
        val items = children(element)
        val map = mutableMapOf<String, Element>()
        for (i in 0 until items.size - 1 step 2) {
            if (items[i].tagName == "key") map[items[i].textContent.trim()] = items[i + 1]
        }
        return map
    }

    private fun number(element: Element): Double? = when (element.tagName) {
        "real", "integer" -> element.textContent.trim().toDoubleOrNull()
        else -> null
    }

    private fun children(element: Element): List<Element> {
        val nodes = element.childNodes
        return (0 until nodes.length).mapNotNull { nodes.item(it) as? Element }
    }
}
