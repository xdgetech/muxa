package tech.xdge.muxa.v2

/**
 * tmux key names, as `KeyMap` produces them (`C-c`, `M-x`, `S-Up`, `F5`,
 * `BSpace`), turned into the bytes an xterm sends. Protocol v2 carries bytes;
 * the key names stay the app's vocabulary for keys.
 */
object KeyBytes {
    /** The bytes for one key name; null for a name no terminal sends. */
    fun bytes(name: String): ByteArray? {
        var rest = name
        var ctrl = false
        var alt = false
        var shift = false
        while (rest.length > 2 && rest[1] == '-' && rest[0] in "CMS") {
            when (rest[0]) {
                'C' -> ctrl = true
                'M' -> alt = true
                else -> shift = true
            }
            rest = rest.substring(2)
        }
        val mod = 1 + (if (shift) 1 else 0) + (if (alt) 2 else 0) + (if (ctrl) 4 else 0)
        special(rest, mod)?.let { return it.toByteArray(Charsets.UTF_8) }
        // One character after the modifiers.
        var c = when {
            rest == "Space" -> 0x20
            rest.length == 1 && rest[0].code < 0x80 -> rest[0].code
            else -> return null
        }
        if (ctrl) {
            c = when (c) {
                0x20, 0x40 -> 0
                0x3f -> 0x7f
                in 0x61..0x7a, in 0x41..0x5f -> c and 0x1f
                else -> return null
            }
        }
        return if (alt) byteArrayOf(0x1b, c.toByte()) else byteArrayOf(c.toByte())
    }

    private fun special(key: String, mod: Int): String? {
        val esc = "\u001b"
        fun csi(final: String) = if (mod == 1) "$esc[$final" else "$esc[1;$mod$final"
        fun tilde(n: Int) = if (mod == 1) "$esc[$n~" else "$esc[$n;$mod~"
        fun ss3(final: String) = if (mod == 1) "${esc}O$final" else "$esc[1;$mod$final"
        val alt = ((mod - 1) and 2) != 0
        return when (key) {
            "Up" -> csi("A")
            "Down" -> csi("B")
            "Right" -> csi("C")
            "Left" -> csi("D")
            "Home" -> csi("H")
            "End" -> csi("F")
            "PageUp" -> tilde(5)
            "PageDown" -> tilde(6)
            "IC" -> tilde(2)
            "DC" -> tilde(3)
            "Enter" -> if (alt) "$esc\r" else "\r"
            "Escape" -> esc
            "Tab" -> "\t"
            "BTab" -> "$esc[Z"
            "BSpace" -> if (alt) "$esc\u007f" else "\u007f"
            "F1" -> ss3("P")
            "F2" -> ss3("Q")
            "F3" -> ss3("R")
            "F4" -> ss3("S")
            else -> FUNCTION[key]?.let(::tilde)
        }
    }

    private val FUNCTION = mapOf(
        "F5" to 15, "F6" to 17, "F7" to 18, "F8" to 19, "F9" to 20, "F10" to 21, "F11" to 23, "F12" to 24,
    )
}
