package tech.xdge.muxa.session

/**
 * Every color the apps draw, by meaning (ADR 0091, appendix), in the order of
 * Swift's `ThemeToken`. The key is the name in theme files; never rename one.
 * Tokens are derived in this order, and each rule reads only tokens above it.
 */
enum class ThemeToken {
    // Base: from the terminal palette.
    terminalBackground, terminalForeground, ground, text,
    surface, card, raised, border, textSecondary, textTertiary,
    accent, working, needsYou, done, idle, failure, decide,

    // Component: from the base tokens.
    pane, cardRaised, pill, selected, rowSelected, borderStrong, rowBorder, buttonBorder,
    divider, rowDivider, cardDivider, textMid, textDim,
    onAccent, onNeedsYou, bannerBack, bannerBorder, promptBack, promptBorder,
    destructiveSolid, destructiveBack, destructiveBorder, warnBack, warnBorder, warnText,
    infoBack, infoBorder, infoText, readText, readBack, writeText, writeBack,
    okText, okBack, failedText, decideBack, todoText, todoBack, userBubble,
    swipeDone, swipeLater, diffAdded, diffRemoved;

    val key: String get() = name
    val isBase: Boolean get() = ordinal < pane.ordinal

    /** The rule of the appendix's table. [v] reads a token computed earlier. */
    internal fun derive(palette: ThemePalette, v: (ThemeToken) -> Rgb): Rgb {
        val ground = v(ThemeToken.ground)
        val text = v(ThemeToken.text)
        fun shade(p: Int) = ground.mixed(text, p)
        fun tint(color: ThemeToken, p: Int) = ground.mixed(v(color), p)
        return when (this) {
            terminalBackground, ThemeToken.ground -> palette.background
            terminalForeground, ThemeToken.text -> palette.foreground
            surface -> shade(3)
            card -> shade(4)
            raised -> shade(8)
            border -> shade(11)
            textSecondary -> dim(text, ground, 31, 4.5)
            textTertiary -> dim(text, ground, 42, 3.0)
            accent -> lift(palette.ansi(4), text, ground, 3.0)
            working -> lift(palette.ansi(2), text, ground, 3.0)
            needsYou -> lift(palette.ansi(3), text, ground, 3.0)
            done -> v(accent)
            idle -> v(textTertiary)
            failure -> lift(palette.ansi(1), text, ground, 4.5)
            decide -> lift(palette.ansi(5), text, ground, 3.0)
            pane -> shade(2)
            cardRaised -> shade(5)
            pill -> shade(7)
            selected, borderStrong -> shade(13)
            rowSelected, cardDivider -> shade(9)
            rowBorder -> shade(18)
            buttonBorder -> shade(15)
            divider -> shade(8)
            rowDivider -> shade(6)
            textMid -> text.mixed(ground, 12)
            textDim -> text.mixed(ground, 60)
            onAccent -> ground
            onNeedsYou -> v(needsYou).mixed(Rgb.BLACK, 90)
            bannerBack -> tint(needsYou, 6)
            bannerBorder -> tint(needsYou, 22)
            promptBack -> tint(needsYou, 12)
            promptBorder -> tint(needsYou, 28)
            destructiveSolid -> v(failure).mixed(Rgb.BLACK, 20)
            destructiveBack -> tint(failure, 12)
            destructiveBorder -> tint(failure, 22)
            warnBack -> tint(failure, 9)
            warnBorder -> tint(failure, 18)
            warnText -> v(failure).mixed(text, 40)
            infoBack -> tint(accent, 6)
            infoBorder -> tint(accent, 14)
            infoText -> v(accent).mixed(text, 70)
            readText -> v(accent).mixed(text, 30)
            readBack -> tint(accent, 12)
            writeText -> v(needsYou).mixed(text, 30)
            writeBack -> v(promptBack)
            okText -> v(working).mixed(text, 35)
            okBack -> tint(working, 12)
            failedText, diffRemoved -> v(failure)
            decideBack -> tint(decide, 12)
            todoText -> v(textMid)
            todoBack -> v(rowSelected)
            userBubble -> v(raised)
            swipeDone -> v(working).mixed(ground, 30)
            swipeLater -> v(decide).mixed(ground, 45)
            diffAdded -> v(working)
        }
    }

    companion object {
        fun of(key: String): ThemeToken? = entries.firstOrNull { it.name == key }

        /** [text] mixed toward the ground, backed off 1% at a time until it keeps [min] contrast. */
        internal fun dim(text: Rgb, ground: Rgb, percent: Int, min: Double): Rgb {
            for (p in percent downTo 0) {
                val c = text.mixed(ground, p)
                if (c.contrast(ground) >= min) return c
            }
            return text
        }

        /** [color] mixed toward [text] 1% at a time until it has [min] contrast on the ground. */
        internal fun lift(color: Rgb, text: Rgb, ground: Rgb, min: Double): Rgb {
            for (p in 0..100) {
                val c = color.mixed(text, p)
                if (c.contrast(ground) >= min) return c
            }
            return text
        }
    }
}

/** A terminal palette as the derivation reads it. */
internal class ThemePalette(val background: Rgb, val foreground: Rgb, val ansi: List<Rgb>?) {
    fun ansi(index: Int): Rgb = ansi?.takeIf { it.size == 16 }?.get(index) ?: FALLBACK[index] ?: foreground

    companion object {
        /** ANSI 1–5 when a palette has none: muxa Dark's status colors. */
        val FALLBACK = mapOf(
            1 to Rgb(0xFF, 0x9B, 0x9B), 2 to Rgb(0x3F, 0xC7, 0x9A), 3 to Rgb(0xF5, 0xA5, 0x24),
            4 to Rgb(0x6A, 0xA6, 0xFF), 5 to Rgb(0xCD, 0xB6, 0xFF),
        )
    }
}
