package tech.xdge.muxa.terminal

/** A color as the program set it. The view picks the actual colors. */
sealed interface TermColor {
    data object Default : TermColor

    /** One of the 256 indexed colors. */
    data class Indexed(val index: Int) : TermColor
    data class Rgb(val r: Int, val g: Int, val b: Int) : TermColor
}

data class CellStyle(
    val foreground: TermColor = TermColor.Default,
    val background: TermColor = TermColor.Default,
    val bold: Boolean = false,
    val italic: Boolean = false,
    val dim: Boolean = false,
    val underline: Boolean = false,
    val inverse: Boolean = false,
    val hidden: Boolean = false,
)

/**
 * Cells of one style on one row. [text] has one char per column, from [column] on.
 * [ScreenSnapshot.WIDE_TAIL] fills the second column of a double-width character.
 */
class Run(val column: Int, val text: String, val style: CellStyle) {
    // Never the text: pane content stays out of logs.
    override fun toString() = "Run($column, ${text.length})"
}

/** What a pane view draws: the grid at its real size and the cursor (zero-based). */
class ScreenSnapshot(
    val cols: Int,
    val rows: Int,
    val lines: List<List<Run>>,
    val cursorRow: Int,
    val cursorColumn: Int,
    val cursorVisible: Boolean,
) {
    companion object {
        const val WIDE_TAIL = ''
        val EMPTY = ScreenSnapshot(0, 0, emptyList(), 0, 0, false)
    }
}
