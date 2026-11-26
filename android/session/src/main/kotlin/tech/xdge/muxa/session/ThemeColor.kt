package tech.xdge.muxa.session

import kotlin.math.max
import kotlin.math.min
import kotlin.math.pow

/** An sRGB color, 8 bits a channel. */
data class Rgb(val r: Int, val g: Int, val b: Int) {
    init {
        require(r in 0..255 && g in 0..255 && b in 0..255) { "channels are 0–255" }
    }

    val hex: String get() = "#%02x%02x%02x".format(r, g, b)

    /**
     * Moves each channel toward [other] by [percent] (0–100), rounded half up.
     * Integer math, as in Swift's `RGB.mixed`, so both give the same bytes (ADR 0091).
     */
    fun mixed(other: Rgb, percent: Int): Rgb {
        val p = percent.coerceIn(0, 100)
        fun channel(a: Int, b: Int) = (a * (100 - p) + b * p + 50) / 100
        return Rgb(channel(r, other.r), channel(g, other.g), channel(b, other.b))
    }

    /** WCAG 2 relative luminance, from 0 (black) to 1 (white). */
    val luminance: Double get() = 0.2126 * LINEAR[r] + 0.7152 * LINEAR[g] + 0.0722 * LINEAR[b]

    /** WCAG 2 contrast ratio, from 1 to 21. */
    fun contrast(other: Rgb): Double {
        val (a, b) = luminance to other.luminance
        return (max(a, b) + 0.05) / (min(a, b) + 0.05)
    }

    companion object {
        val BLACK = Rgb(0, 0, 0)

        /** `#1d1f21` or `1d1f21`; null for anything else. */
        fun parse(hex: String): Rgb? {
            val digits = hex.removePrefix("#")
            if (digits.length != 6) return null
            val v = digits.toIntOrNull(16) ?: return null
            return Rgb(v shr 16 and 0xFF, v shr 8 and 0xFF, v and 0xFF)
        }

        private val LINEAR = DoubleArray(256) { byte ->
            val c = byte / 255.0
            if (c <= 0.04045) c / 12.92 else ((c + 0.055) / 1.055).pow(2.4)
        }
    }
}
