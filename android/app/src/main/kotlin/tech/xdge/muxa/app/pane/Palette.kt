package tech.xdge.muxa.app.pane

import android.graphics.Color
import tech.xdge.muxa.terminal.TermColor

/** The terminal's colors: xterm's 256, on a black background. */
object Palette {
    val BACKGROUND = Color.BLACK
    private val FOREGROUND = Color.rgb(229, 229, 229)

    private val base = intArrayOf(
        Color.rgb(0, 0, 0), Color.rgb(205, 49, 49), Color.rgb(13, 188, 121), Color.rgb(229, 229, 16),
        Color.rgb(36, 114, 200), Color.rgb(188, 63, 188), Color.rgb(17, 168, 205), Color.rgb(229, 229, 229),
        Color.rgb(102, 102, 102), Color.rgb(241, 76, 76), Color.rgb(35, 209, 139), Color.rgb(245, 245, 67),
        Color.rgb(59, 142, 234), Color.rgb(214, 112, 214), Color.rgb(41, 184, 219), Color.rgb(255, 255, 255),
    )

    private val table = IntArray(256) { i ->
        when {
            i < 16 -> base[i]
            i < 232 -> {
                val n = i - 16
                fun level(v: Int) = if (v == 0) 0 else 55 + v * 40
                Color.rgb(level(n / 36), level(n / 6 % 6), level(n % 6))
            }
            else -> (8 + (i - 232) * 10).let { Color.rgb(it, it, it) }
        }
    }

    /** Bold makes the eight basic colors bright, as most terminals do. */
    fun foreground(c: TermColor, bold: Boolean): Int = when (c) {
        TermColor.Default -> FOREGROUND
        is TermColor.Indexed -> table[if (bold && c.index < 8) c.index + 8 else c.index.coerceIn(0, 255)]
        is TermColor.Rgb -> Color.rgb(c.r, c.g, c.b)
    }

    fun background(c: TermColor): Int = when (c) {
        TermColor.Default -> BACKGROUND
        is TermColor.Indexed -> table[c.index.coerceIn(0, 255)]
        is TermColor.Rgb -> Color.rgb(c.r, c.g, c.b)
    }

    fun dim(color: Int): Int = Color.argb(160, Color.red(color), Color.green(color), Color.blue(color))
}
