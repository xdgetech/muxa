package tech.xdge.muxa.app.pane

import tech.xdge.muxa.session.Rgb
import tech.xdge.muxa.session.TerminalPalette
import tech.xdge.muxa.terminal.TermColor

/**
 * A pane's colors: xterm's 256 on the theme's terminal ground (ADR 0091). The
 * theme sets the first 16 when it has them; the cube and the grays stay xterm's.
 */
class TerminalColors(palette: TerminalPalette) {
    val background: Int = palette.background.opaque()
    val foreground: Int = palette.foreground.opaque()
    val cursor: Int = palette.cursor?.let { argb(CURSOR_ALPHA, it.r, it.g, it.b) } ?: CURSOR

    private val table = IntArray(256) { i ->
        when {
            i < 16 -> palette.ansi?.getOrNull(i)?.opaque() ?: BASE[i]
            i < 232 -> {
                val n = i - 16
                fun level(v: Int) = if (v == 0) 0 else 55 + v * 40
                rgb(level(n / 36), level(n / 6 % 6), level(n % 6))
            }
            else -> (8 + (i - 232) * 10).let { rgb(it, it, it) }
        }
    }

    /** Bold makes the eight basic colors bright, as most terminals do. */
    fun foreground(c: TermColor, bold: Boolean): Int = when (c) {
        TermColor.Default -> foreground
        is TermColor.Indexed -> table[if (bold && c.index < 8) c.index + 8 else c.index.coerceIn(0, 255)]
        is TermColor.Rgb -> rgb(c.r, c.g, c.b)
    }

    fun background(c: TermColor): Int = when (c) {
        TermColor.Default -> background
        is TermColor.Indexed -> table[c.index.coerceIn(0, 255)]
        is TermColor.Rgb -> rgb(c.r, c.g, c.b)
    }

    companion object {
        // android.graphics.Color's functions are stubs in JVM tests, so the bits are packed here.
        fun argb(a: Int, r: Int, g: Int, b: Int): Int = (a shl 24) or (r shl 16) or (g shl 8) or b
        fun rgb(r: Int, g: Int, b: Int): Int = argb(0xFF, r, g, b)
        private fun Rgb.opaque(): Int = rgb(r, g, b)

        /** The cursor lets the character under it show through. */
        private const val CURSOR_ALPHA = 160
        private val CURSOR = argb(CURSOR_ALPHA, 220, 220, 220)

        /** The view's own 16 colors, for a theme without any. */
        private val BASE = intArrayOf(
            rgb(0, 0, 0), rgb(205, 49, 49), rgb(13, 188, 121), rgb(229, 229, 16),
            rgb(36, 114, 200), rgb(188, 63, 188), rgb(17, 168, 205), rgb(229, 229, 229),
            rgb(102, 102, 102), rgb(241, 76, 76), rgb(35, 209, 139), rgb(245, 245, 67),
            rgb(59, 142, 234), rgb(214, 112, 214), rgb(41, 184, 219), rgb(255, 255, 255),
        )

        fun dim(color: Int): Int = (160 shl 24) or (color and 0xFFFFFF)
    }
}
