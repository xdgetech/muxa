package tech.xdge.muxa.terminal

import com.jediterm.core.util.TermSize
import com.jediterm.terminal.CursorShape
import com.jediterm.terminal.TerminalColor
import com.jediterm.terminal.TerminalDisplay
import com.jediterm.terminal.TextStyle
import com.jediterm.terminal.TtyBasedArrayDataStream
import com.jediterm.terminal.TtyConnector
import com.jediterm.terminal.emulator.JediEmulator
import com.jediterm.terminal.emulator.mouse.MouseFormat
import com.jediterm.terminal.emulator.mouse.MouseMode
import com.jediterm.terminal.model.JediTerminal
import com.jediterm.terminal.model.StyleState
import com.jediterm.terminal.model.TerminalSelection
import com.jediterm.terminal.model.TerminalTextBuffer
import java.nio.ByteBuffer
import java.nio.CharBuffer
import java.nio.charset.CodingErrorAction
import java.util.concurrent.TimeUnit
import java.util.concurrent.locks.ReentrantLock
import kotlin.concurrent.withLock

/**
 * A pane's screen on JediTerm, with no view. The grid is exactly the pane's size and
 * never follows a view (ADR 0004). Replies the emulator would send to the host are
 * dropped: the bridge's side answers those itself.
 *
 * JediTerm parses on its own thread, which blocks for input. [onIdle] runs there each
 * time every fed byte is parsed, so a view redraws once per batch.
 */
class PaneScreen(private val onIdle: () -> Unit = {}) : PaneRenderer, AutoCloseable {
    @Volatile
    private var engine: Engine? = null

    override fun reset(cols: Int, rows: Int, data: ByteArray) {
        val next = Engine(cols.coerceAtLeast(1), rows.coerceAtLeast(1), onIdle)
        engine?.close()
        engine = next
        next.feed(data)
    }

    override fun feed(data: ByteArray) {
        engine?.feed(data)
    }

    /** Waits until every fed byte is parsed. False on timeout. */
    fun awaitIdle(timeoutMillis: Long = 5_000): Boolean = engine?.awaitIdle(timeoutMillis) ?: true

    fun snapshot(): ScreenSnapshot = engine?.snapshot() ?: ScreenSnapshot.EMPTY

    /** Visible rows with trailing spaces and trailing empty rows removed. */
    fun rows(): List<String> {
        val rows = snapshot().lines.map { runs ->
            // Counted in columns: a wide character's tail takes a column but adds no char.
            var column = 0
            buildString {
                for (run in runs) {
                    while (column < run.column) {
                        append(' ')
                        column++
                    }
                    for (c in run.text) {
                        if (c != ScreenSnapshot.WIDE_TAIL) append(if (c == '\u0000') ' ' else c)
                        column++
                    }
                }
            }.trimEnd(' ')
        }
        return rows.dropLastWhile { it.isEmpty() }
    }

    override fun close() {
        engine?.close()
        engine = null
    }

    private class Engine(val cols: Int, val rows: Int, onIdle: () -> Unit) : AutoCloseable {
        private val display = Display()
        private val buffer = TerminalTextBuffer(cols, rows, StyleState(), HISTORY)
        private val terminal = JediTerminal(display, buffer, StyleState())
        private val input = Input(onIdle)
        private val thread = Thread({ run() }, "muxa-pane").apply { isDaemon = true }

        init {
            thread.start()
        }

        fun feed(data: ByteArray) = input.feed(data)

        fun awaitIdle(timeoutMillis: Long) = input.awaitIdle(timeoutMillis)

        private fun run() {
            val emulator = JediEmulator(TtyBasedArrayDataStream(input), terminal)
            try {
                while (emulator.hasNext()) emulator.next()
            } catch (_: Throwable) {
                // A parser failure ends this screen; the next `screen` builds a new one.
                // Nothing is logged: the failure can carry pane bytes.
            } finally {
                input.close()
            }
        }

        fun snapshot(): ScreenSnapshot {
            buffer.lock()
            try {
                val lines = (0 until rows).map { row ->
                    val runs = mutableListOf<Run>()
                    var column = 0
                    for (entry in buffer.getLine(row).entries) {
                        if (entry.isNul) break
                        val text = entry.text.toString()
                        runs += Run(column, text, style(entry.style))
                        column += text.length
                    }
                    runs
                }
                return ScreenSnapshot(
                    cols, rows, lines,
                    cursorRow = terminal.cursorY - 1,
                    cursorColumn = (terminal.cursorX - 1).coerceIn(0, cols - 1),
                    cursorVisible = display.showsCursor,
                )
            } finally {
                buffer.unlock()
            }
        }

        override fun close() = input.close()
    }

    /** The bytes fed so far, decoded as UTF-8 across feeds. JediTerm reads chars. */
    private class Input(private val onIdle: () -> Unit) : TtyConnector {
        private val lock = ReentrantLock()
        private val changed = lock.newCondition()
        private val pending = StringBuilder()
        private val decoder = Charsets.UTF_8.newDecoder()
            .onMalformedInput(CodingErrorAction.REPLACE)
            .onUnmappableCharacter(CodingErrorAction.REPLACE)

        // A UTF-8 sequence cut between two feeds waits here for its end.
        private var carry: ByteBuffer = ByteBuffer.allocate(0)
        private var waiting = false
        private var closed = false

        fun feed(data: ByteArray) {
            lock.withLock { append(data) }
        }

        private fun append(data: ByteArray) {
            if (closed) return
            val bytes = ByteBuffer.allocate(carry.remaining() + data.size).put(carry).put(data)
            bytes.flip()
            val chars = CharBuffer.allocate(bytes.remaining() + 1)
            decoder.decode(bytes, chars, false)
            carry = ByteBuffer.allocate(bytes.remaining()).put(bytes)
            carry.flip()
            chars.flip()
            pending.append(chars)
            changed.signalAll()
        }

        fun awaitIdle(timeoutMillis: Long): Boolean = lock.withLock {
            val deadline = System.nanoTime() + TimeUnit.MILLISECONDS.toNanos(timeoutMillis)
            while (!closed && !(waiting && pending.isEmpty())) {
                val left = deadline - System.nanoTime()
                if (left <= 0) return false
                changed.awaitNanos(left)
            }
            true
        }

        override fun read(buf: CharArray, offset: Int, length: Int): Int {
            val idle = lock.withLock { pending.isEmpty() && !closed }
            // Outside the lock: the view reads the buffer from its own thread.
            if (idle) onIdle()
            lock.withLock {
                while (pending.isEmpty() && !closed) {
                    waiting = true
                    changed.signalAll()
                    changed.await()
                }
                waiting = false
                if (pending.isEmpty()) return -1
                val n = minOf(length, pending.length)
                for (i in 0 until n) buf[offset + i] = pending[i]
                pending.delete(0, n)
                return n
            }
        }

        override fun ready(): Boolean = lock.withLock { pending.isNotEmpty() }

        override fun close() = lock.withLock {
            closed = true
            changed.signalAll()
        }

        override fun isConnected() = lock.withLock { !closed }
        override fun write(bytes: ByteArray) {}
        override fun write(string: String) {}
        override fun resize(termSize: TermSize) {}
        override fun waitFor() = 0
        override fun getName() = "pane"
    }

    private class Display : TerminalDisplay {
        @Volatile
        var showsCursor = true
        private var title = ""

        override fun setCursor(x: Int, y: Int) {}
        override fun setCursorShape(cursorShape: CursorShape?) {}
        override fun beep() {}
        override fun scrollArea(scrollRegionTop: Int, scrollRegionSize: Int, dy: Int) {}
        override fun setCursorVisible(isCursorVisible: Boolean) {
            showsCursor = isCursorVisible
        }
        override fun useAlternateScreenBuffer(useAlternateScreenBuffer: Boolean) {}
        override fun getWindowTitle() = title
        override fun setWindowTitle(windowTitle: String) {
            title = windowTitle
        }
        override fun getSelection(): TerminalSelection? = null
        override fun terminalMouseModeSet(mouseMode: MouseMode) {}
        override fun setMouseFormat(mouseFormat: MouseFormat) {}
        override fun ambiguousCharsAreDoubleWidth() = false
    }

    private companion object {
        // The pane has no scrollback of its own; the host keeps the history.
        const val HISTORY = 0

        fun style(s: TextStyle) = CellStyle(
            foreground = color(s.foreground),
            background = color(s.background),
            bold = s.hasOption(TextStyle.Option.BOLD),
            italic = s.hasOption(TextStyle.Option.ITALIC),
            dim = s.hasOption(TextStyle.Option.DIM),
            underline = s.hasOption(TextStyle.Option.UNDERLINED),
            inverse = s.hasOption(TextStyle.Option.INVERSE),
            hidden = s.hasOption(TextStyle.Option.HIDDEN),
        )

        fun color(c: TerminalColor?): TermColor = when {
            c == null -> TermColor.Default
            c.isIndexed -> TermColor.Indexed(c.colorIndex)
            else -> c.toColor().let { TermColor.Rgb(it.red, it.green, it.blue) }
        }
    }
}
