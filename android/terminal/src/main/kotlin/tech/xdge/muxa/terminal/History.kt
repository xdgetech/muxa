package tech.xdge.muxa.terminal

/**
 * A pane's scrollback from a `history` message, as lines of styled runs. The emulator is as
 * tall as the history, so no line scrolls away while it is read.
 */
object History {
    private const val NEWLINE = '\n'.code.toByte()
    private const val RETURN = '\r'.code.toByte()
    private const val PARSE_MILLIS = 10_000L

    /** [data]'s lines, oldest first: each one [cols] wide, ending in `\n`. */
    fun lines(cols: Int, data: ByteArray): List<List<Run>> {
        val count = data.count { it == NEWLINE }
        if (count == 0) return emptyList()
        return PaneScreen().use { screen ->
            // One more row, so the last newline does not scroll the first line out.
            screen.reset(cols, count + 1, withReturns(data))
            screen.awaitIdle(PARSE_MILLIS)
            screen.snapshot().lines.take(count)
        }
    }

    /** tmux ends each line with `\n` alone; an emulator also needs `\r` to start the next one at column 0. */
    private fun withReturns(data: ByteArray): ByteArray {
        val out = java.io.ByteArrayOutputStream(data.size + data.size / 40)
        for (b in data) {
            if (b == NEWLINE) out.write(RETURN.toInt())
            out.write(b.toInt())
        }
        return out.toByteArray()
    }
}
