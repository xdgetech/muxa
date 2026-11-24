package tech.xdge.muxa.session

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.SerializationException
import kotlinx.serialization.json.Json

enum class ThemeAppearance {
    @SerialName("dark") DARK,
    @SerialName("light") LIGHT,
    ;

    companion object {
        fun of(value: String?): ThemeAppearance? = when (value) {
            "dark" -> DARK
            "light" -> LIGHT
            else -> null
        }
    }
}

/**
 * One theme as a file (ADR 0091), as Swift's `ThemeFile`: partial, kept as
 * written, resolved by [ThemeRegistry]. Unknown keys and token names are
 * ignored, so a file written for a newer app still loads.
 */
@Serializable
data class ThemeFile(
    val schema: Int,
    val id: String,
    val name: String,
    val extends: String? = null,
    /** `dark` or `light`; any other value is ignored. */
    val appearance: String? = null,
    val terminal: Terminal? = null,
    /** Token key → `#rrggbb`. */
    val tokens: Map<String, String>? = null,
) {
    @Serializable
    data class Terminal(
        val background: String,
        val foreground: String,
        val cursor: String? = null,
        val selection: String? = null,
        /** The 16 ANSI colors, or none. */
        val ansi: List<String>? = null,
    )

    /** Why a theme file was refused; the message says what to fix. */
    class LoadError(val reason: Reason, val detail: String = "") : Exception(reason.message(detail)) {
        enum class Reason {
            NOT_JSON, NEWER_SCHEMA, BAD_ID, NO_NAME, NO_TERMINAL, BAD_COLOR;

            fun message(detail: String): String = when (this) {
                NOT_JSON -> "The theme is not a muxa theme file (JSON)."
                NEWER_SCHEMA -> "The theme needs a newer muxa (theme schema $detail)."
                BAD_ID -> "The theme's id \"$detail\" may use only letters, digits, \".\", \"_\" and \"-\"."
                NO_NAME -> "The theme has no name."
                NO_TERMINAL -> "The theme has no terminal colors and extends no other theme."
                BAD_COLOR -> "The theme's $detail is not a #rrggbb color."
            }
        }
    }

    internal fun check() {
        if (schema > SCHEMA_VERSION) throw LoadError(LoadError.Reason.NEWER_SCHEMA, "$schema")
        if (!validId(id)) throw LoadError(LoadError.Reason.BAD_ID, id)
        if (name.isBlank()) throw LoadError(LoadError.Reason.NO_NAME)
        if (terminal == null && extends == null) throw LoadError(LoadError.Reason.NO_TERMINAL)
        terminal?.let { t ->
            listOf("background" to t.background, "foreground" to t.foreground, "cursor" to t.cursor, "selection" to t.selection)
                .forEach { (key, value) ->
                    if (value != null && Rgb.parse(value) == null) throw LoadError(LoadError.Reason.BAD_COLOR, "terminal $key")
                }
            t.ansi?.let { ansi ->
                if (ansi.size != 16) throw LoadError(LoadError.Reason.BAD_COLOR, "terminal ansi (needs 16)")
                ansi.forEachIndexed { i, value ->
                    if (Rgb.parse(value) == null) throw LoadError(LoadError.Reason.BAD_COLOR, "terminal ansi $i")
                }
            }
        }
        tokens.orEmpty().forEach { (key, value) ->
            if (ThemeToken.of(key) != null && Rgb.parse(value) == null) throw LoadError(LoadError.Reason.BAD_COLOR, key)
        }
    }

    /** This file with [token] set to [color], or back to its parent's when null. */
    fun setting(token: ThemeToken, color: Rgb?): ThemeFile {
        val next = tokens.orEmpty().toMutableMap()
        if (color == null) next.remove(token.key) else next[token.key] = color.hex
        return copy(tokens = next.takeIf { it.isNotEmpty() })
    }

    fun encoded(): String = writer.encodeToString(serializer(), this)

    companion object {
        /** Raised only for a change older apps must not read. */
        const val SCHEMA_VERSION = 1

        private val reader = Json { ignoreUnknownKeys = true }
        private val writer = Json { prettyPrint = true; explicitNulls = false }

        /** Reads and checks a theme file. */
        fun parse(text: String): ThemeFile {
            val file = try {
                reader.decodeFromString(serializer(), text)
            } catch (e: SerializationException) {
                throw LoadError(LoadError.Reason.NOT_JSON)
            } catch (e: IllegalArgumentException) {
                throw LoadError(LoadError.Reason.NOT_JSON)
            }
            file.check()
            return file
        }

        /** Ids name files in the user's theme folder. */
        internal fun validId(id: String): Boolean =
            id.isNotEmpty() && id.length <= 100 && !id.startsWith(".") &&
                id.all { it in 'a'..'z' || it in 'A'..'Z' || it in '0'..'9' || it in "._-" }
    }
}
