package tech.xdge.muxa.session

/** How a pane's grid is fitted to the view. No mode changes the grid (ADR 0004). */
enum class PaneFit {
    /** Every row and column in the view. */
    WHOLE,

    /** The columns fill the width. The grid can be taller than the view, which scrolls. */
    WIDTH,

    /**
     * The font never goes below [MINIMUM_FONT_SIZE]. The grid can be wider and taller
     * than the view, which scrolls on both axes.
     */
    READABLE;

    /** The font size this mode uses, from the largest fonts at which the whole grid, and the columns alone, fit. */
    fun fontSize(wholeFont: Float, widthFont: Float): Float = when (this) {
        WHOLE -> wholeFont
        WIDTH -> widthFont
        READABLE -> maxOf(widthFont, MINIMUM_FONT_SIZE)
    }

    companion object {
        /** The smallest font a phone screen shows legibly, in sp (ADR 0012). */
        const val MINIMUM_FONT_SIZE = 9f

        /** The mode a view picks for itself. Wide views fit the width. */
        fun automatic(wide: Boolean, wholeFont: Float, widthFont: Float): PaneFit = when {
            !wide && wholeFont >= MINIMUM_FONT_SIZE -> WHOLE
            widthFont >= MINIMUM_FONT_SIZE -> WIDTH
            else -> READABLE
        }

        /** A pinch: in for the whole pane, out for a readable font. */
        fun pinched(scale: Float): PaneFit? = when {
            scale < 0.8f -> WHOLE
            scale > 1.25f -> READABLE
            else -> null
        }
    }
}
