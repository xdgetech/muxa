package tech.xdge.muxa.session

/** What a key press or the input method sends to the pane. */
sealed interface KeyOutput {
    /** Named keys for the bridge's `keys` request. */
    data class Keys(val keys: List<String>) : KeyOutput

    /** Typed literally. */
    data class Text(val text: String) : KeyOutput {
        // Never the text: typed input stays out of logs.
        override fun toString() = "Text(${text.length})"
    }

    /** Not for the pane: an app shortcut, or a key that has no name. */
    data object None : KeyOutput
}

/**
 * Turns a hardware key press into what the bridge sends. Named keys go to `send-keys`
 * on the host, which encodes them for the pane's modes; text is typed literally.
 */
object KeyMap {
    sealed interface Special {
        data object Up : Special
        data object Down : Special
        data object Left : Special
        data object Right : Special
        data object Home : Special
        data object End : Special
        data object PageUp : Special
        data object PageDown : Special
        data object Enter : Special
        data object Escape : Special
        data object Tab : Special
        data object Backspace : Special
        data object DeleteForward : Special
        data object Insert : Special
        data class Function(val n: Int) : Special
    }

    data class Modifiers(
        val control: Boolean = false,
        val alt: Boolean = false,
        val shift: Boolean = false,
        /** An app shortcut, never for the pane (Ctrl+Shift on Android, see ADR 0013). */
        val shortcut: Boolean = false,
    ) {
        companion object {
            val NONE = Modifiers()
            val CONTROL = Modifiers(control = true)
        }
    }

    /** [characters] is what the key types without modifiers applied. */
    fun map(special: Special?, characters: String, modifiers: Modifiers): KeyOutput {
        if (modifiers.shortcut) return KeyOutput.None
        if (special != null) {
            val name = name(special, modifiers.shift) ?: return KeyOutput.None
            return KeyOutput.Keys(listOf(prefix(modifiers, shiftInName = special == Special.Tab) + name))
        }
        if (!modifiers.control && !modifiers.alt) {
            return if (characters.isEmpty()) KeyOutput.None else KeyOutput.Text(characters)
        }
        // The host names one printable ASCII character after the modifiers.
        val c = characters.singleOrNull()
        if (c == null || c.code < 0x21 || c.code >= 0x7f) {
            return if (characters == " ") KeyOutput.Keys(listOf(prefix(modifiers, false) + "Space")) else KeyOutput.None
        }
        // Shift is already in the character (`A`, `!`); naming it again changes the key.
        return KeyOutput.Keys(listOf(prefix(modifiers.copy(shift = false), false) + characters))
    }

    private fun prefix(m: Modifiers, shiftInName: Boolean) = buildString {
        if (m.control) append("C-")
        if (m.alt) append("M-")
        if (m.shift && !shiftInName) append("S-")
    }

    private fun name(key: Special, shift: Boolean): String? = when (key) {
        Special.Up -> "Up"
        Special.Down -> "Down"
        Special.Left -> "Left"
        Special.Right -> "Right"
        Special.Home -> "Home"
        Special.End -> "End"
        Special.PageUp -> "PageUp"
        Special.PageDown -> "PageDown"
        Special.Enter -> "Enter"
        Special.Escape -> "Escape"
        Special.Tab -> if (shift) "BTab" else "Tab"
        Special.Backspace -> "BSpace"
        Special.DeleteForward -> "DC"
        Special.Insert -> "IC"
        is Special.Function -> if (key.n in 1..12) "F${key.n}" else null
    }
}

/**
 * The keys above the on-screen keyboard, which cannot type them itself.
 * Ctrl is one-shot: it applies to the next key or character, then releases.
 */
data class KeyBar(val control: Boolean = false) {
    enum class Key { ESCAPE, TAB, BACK_TAB, CONTROL, LEFT, DOWN, UP, RIGHT, INTERRUPT }

    /** A tap on one of the bar's keys: the new bar, and what to send. */
    fun tap(key: Key): Pair<KeyBar, KeyOutput> {
        val mods = if (control) KeyMap.Modifiers.CONTROL else KeyMap.Modifiers.NONE
        val output = when (key) {
            Key.CONTROL -> return KeyBar(!control) to KeyOutput.None
            Key.INTERRUPT -> KeyOutput.Keys(listOf("C-c"))
            Key.ESCAPE -> KeyMap.map(KeyMap.Special.Escape, "", mods)
            Key.TAB -> KeyMap.map(KeyMap.Special.Tab, "", mods)
            Key.BACK_TAB -> KeyMap.map(KeyMap.Special.Tab, "", mods.copy(shift = true))
            Key.LEFT -> KeyMap.map(KeyMap.Special.Left, "", mods)
            Key.DOWN -> KeyMap.map(KeyMap.Special.Down, "", mods)
            Key.UP -> KeyMap.map(KeyMap.Special.Up, "", mods)
            Key.RIGHT -> KeyMap.map(KeyMap.Special.Right, "", mods)
        }
        return KeyBar(false) to output
    }

    /**
     * What the keyboard typed, with a pending Ctrl applied. Ctrl names only one ASCII
     * character, so other text goes as typed and still releases it.
     */
    fun type(output: KeyOutput): Pair<KeyBar, KeyOutput> {
        if (!control || output == KeyOutput.None) return this to output
        if (output !is KeyOutput.Text) return KeyBar(false) to output
        val chord = KeyMap.map(null, output.text, KeyMap.Modifiers.CONTROL)
        return KeyBar(false) to if (chord == KeyOutput.None) output else chord
    }
}

/**
 * Text an input method is still composing (as in Japanese kana before conversion).
 * It stays on the device: only committed text reaches the pane.
 */
data class Composition(
    val marked: String = "",
    /** The input method's selection inside [marked], in UTF-16 offsets, end excluded. */
    val selectionStart: Int = 0,
    val selectionEnd: Int = 0,
) {
    val isComposing: Boolean get() = marked.isNotEmpty()

    fun mark(text: String, start: Int, end: Int): Composition =
        Composition(text, start.coerceIn(0, text.length), end.coerceIn(0, text.length))

    /** Ends the composition: what to send is the marked text as typed. */
    fun commit(): Pair<Composition, KeyOutput> =
        Composition() to if (marked.isEmpty()) KeyOutput.None else KeyOutput.Text(marked)

    /** Text that replaces the composition, or plain typing when there is none. */
    fun insert(text: String): Pair<Composition, KeyOutput> = Composition() to when {
        text == "\n" -> KeyOutput.Keys(listOf("Enter"))
        text.isEmpty() -> KeyOutput.None
        else -> KeyOutput.Text(text)
    }

    /** Inside a composition, Backspace edits the composition and sends nothing. */
    fun deleteBackward(): Pair<Composition, KeyOutput> {
        if (!isComposing) return this to KeyOutput.Keys(listOf("BSpace"))
        val text = marked.dropLast(1)
        return Composition(text, text.length, text.length) to KeyOutput.None
    }
}
