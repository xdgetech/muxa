package tech.xdge.muxa.app

import tech.xdge.muxa.terminal.Run

/** The shown pane's scrollback, read once from the host (capability `history`). */
sealed interface PaneHistory {
    data object Loading : PaneHistory

    /** [lines] oldest first, each [cols] wide; [truncated] says the host keeps older ones. */
    data class Loaded(val cols: Int, val lines: List<List<Run>>, val truncated: Boolean) : PaneHistory

    data class Failed(val reason: String) : PaneHistory
}
