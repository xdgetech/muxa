package tech.xdge.muxa.terminal

/** Where a watched pane's bytes go. */
interface PaneRenderer {
    /** Called for every `screen`: a fresh emulator of exactly this size, then [data]. */
    fun reset(cols: Int, rows: Int, data: ByteArray)

    fun feed(data: ByteArray)
}
