package tech.xdge.muxa.session

import kotlin.math.floor

/**
 * The grid a pane area shows at the default font. The bridge sizes the watched pane's
 * window for it (`viewport`, ADR 0020); the pane still renders at the size it gets.
 */
data class PaneViewport(val cols: Int, val rows: Int) {
    /**
     * A pane no larger than this grid fits whole at the default font or larger, so it is
     * shown whole: fitting the width would make it scroll for nothing.
     */
    fun showsWhole(cols: Int, rows: Int): Boolean = cols <= this.cols && rows <= this.rows

    companion object {
        /** The protocol's limits. */
        private val COLS = 10..1000
        private val ROWS = 5..1000

        /**
         * The default font, in sp: a phone-sized window gets more columns from a smaller
         * font, never below the 9 sp floor.
         */
        fun fontSize(compact: Boolean): Float = maxOf(if (compact) 10f else 13f, PaneFit.MINIMUM_FONT_SIZE)

        /**
         * The cells of [cellWidth] x [cellHeight] that fit in [width] x [height], with the
         * one pixel of slack the fit keeps. Null for an area with no room.
         */
        fun fitting(width: Float, height: Float, cellWidth: Float, cellHeight: Float): PaneViewport? {
            if (width <= 1 || height <= 1 || cellWidth <= 0 || cellHeight <= 0) return null
            val cols = floor((width - 1) / cellWidth).toInt()
            val rows = floor((height - 1) / cellHeight).toInt()
            return PaneViewport(cols.coerceIn(COLS), rows.coerceIn(ROWS))
        }
    }
}
