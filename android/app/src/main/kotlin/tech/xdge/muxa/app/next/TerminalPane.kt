package tech.xdge.muxa.app.next

import android.content.res.Configuration
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.ui.draw.clip
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.unit.sp
import tech.xdge.muxa.app.ui.Palette
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Text
import androidx.compose.ui.Alignment
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import tech.xdge.muxa.app.ui.Theme
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.imePadding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clipToBounds
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.viewinterop.AndroidView
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import tech.xdge.muxa.app.pane.TerminalPaneView
import tech.xdge.muxa.session.KeyBar
import tech.xdge.muxa.session.KeyOutput
import tech.xdge.muxa.session.PaneViewport
import tech.xdge.muxa.v2.KeyBytes
import tech.xdge.muxa.v2.V2Event

/**
 * One terminal on protocol v2: the screen at the grid this view fits, and keys
 * sent as bytes. This device is the terminal's last active viewer, so the grid
 * is the terminal's size (ADR 0067 decision 7).
 */
@Composable
fun TerminalPane(model: NextModel, terminal: String, modifier: Modifier = Modifier) {
    var bar by remember { mutableStateOf(KeyBar()) }
    var view by remember { mutableStateOf<TerminalPaneView?>(null) }
    // Bumped on every screen change, so the semantics below carry the current text.
    var content by remember { mutableIntStateOf(0) }
    val link = remember(terminal) { TerminalLink(model, terminal) }
    val hardware = LocalConfiguration.current.hardKeyboardHidden == Configuration.HARDKEYBOARDHIDDEN_NO
    Column(modifier.fillMaxSize().imePadding()) {
        AndroidView(
            factory = { context ->
                TerminalPaneView(context).apply {
                    onOutput = link::send
                    onPaste = { link.send(it.toByteArray()) }
                    onKeyBar = { bar = it }
                    onViewport = link::gridChanged
                    onContent = { content++ }
                    link.view = this
                    view = this
                }
            },
            // The view scrolls to follow the cursor, and Compose does not clip an AndroidView.
            modifier = Modifier.weight(1f).fillMaxWidth().clipToBounds().testTag("focus.pane").semantics {
                contentDescription = "Terminal"
                stateDescription = if (content >= 0) view?.screenText.orEmpty() else ""
            },
        )
        // A hardware keyboard has these keys. The bar stays while typing and while not, so the
        // keyboard's show and hide resize the terminal by its own height only.
        if (!hardware) KeyBarRow(bar) { view?.tap(it) }
    }
    DisposableEffect(link) {
        onDispose {
            link.unbind()
            view?.screen?.close()
        }
    }
}

/** A terminal's channel for one view: attached at the view's grid, again after a lost resume. */
private class TerminalLink(private val model: NextModel, private val terminal: String) {
    var view: TerminalPaneView? = null
    private var channel: Int? = null
    private var grid: PaneViewport? = null
    private var shown: PaneViewport? = null
    private var attaching: Job? = null
    private var ended = false

    fun gridChanged(fit: PaneViewport) {
        if (fit == grid) return
        grid = fit
        attach()
    }

    fun unbind() {
        view = null
        attaching?.cancel()
        channel?.let(model::detach)
        channel = null
    }

    /**
     * Attaches at the current grid. A new grid attaches again, so the server
     * resizes the terminal and paints it at the new size. Waits for the view
     * to settle: the keyboard resizes it several times.
     */
    private fun attach() {
        if (ended) return
        attaching?.cancel()
        attaching = model.scope.launch {
            delay(SETTLE_MS)
            val fit = grid ?: return@launch
            channel?.let(model::detach)
            channel = null
            shown = fit
            channel = model.attach(terminal, fit.cols, fit.rows, ::handle)
            if (view == null) unbind()
        }
    }

    private fun handle(event: V2Event) {
        val view = view ?: return
        val fit = shown ?: return
        when (event) {
            is V2Event.Paint -> view.screen.reset(fit.cols, fit.rows, ByteArray(0))
            is V2Event.Output -> view.screen.feed(event.bytes)
            is V2Event.Closed -> {
                if (event.channel != channel) return
                channel = null
                val exit = event.exit
                if (exit != null) {
                    // The program ended; show why and stay on the last screen.
                    ended = true
                    view.screen.feed("\r\n[${exit.reason}]\r\n".toByteArray())
                } else {
                    // Not resumed after a reconnect: attach again, with a fresh screen.
                    attach()
                }
            }
            else -> {}
        }
    }

    fun send(output: KeyOutput) = when (output) {
        is KeyOutput.Text -> send(output.text.toByteArray())
        is KeyOutput.Keys -> send(output.keys.mapNotNull(KeyBytes::bytes).fold(ByteArray(0)) { a, b -> a + b })
        KeyOutput.None -> {}
    }

    fun send(bytes: ByteArray) {
        val ch = channel ?: return
        if (bytes.isNotEmpty()) model.input(ch, bytes)
    }

    private companion object {
        const val SETTLE_MS = 200L
    }
}

/** The keys above the on-screen keyboard, which cannot type them itself. */
@Composable
private fun KeyBarRow(bar: KeyBar, tap: (KeyBar.Key) -> Unit) {
    Row(
        Modifier.fillMaxWidth().padding(start = 2.dp, end = 2.dp, top = 8.dp, bottom = 2.dp),
        horizontalArrangement = Arrangement.spacedBy(4.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        for (key in KeyBar.Key.entries) {
            val (face, label) = face(key)
            val selected = key == KeyBar.Key.CONTROL && bar.control
            Box(
                Modifier
                    .weight(1f)
                    .height(KEY_HEIGHT.dp)
                    .clip(RoundedCornerShape(8.dp))
                    .background(if (selected) Theme.accent else KEY_FILL)
                    .border(1.dp, Palette.borderStrong, RoundedCornerShape(8.dp))
                    .clickable(role = Role.Button) { tap(key) }
                    .testTag("keybar-$key")
                    .semantics { contentDescription = label },
                contentAlignment = Alignment.Center,
            ) {
                Text(face, fontFamily = FontFamily.Monospace, fontSize = 13.sp, color = if (selected) Palette.ground else Palette.text, maxLines = 1)
            }
        }
    }
}

private fun face(key: KeyBar.Key): Pair<String, String> = when (key) {
    KeyBar.Key.ESCAPE -> "esc" to "Escape"
    KeyBar.Key.TAB -> "⇥" to "Tab"
    KeyBar.Key.BACK_TAB -> "⇤" to "Shift Tab"
    KeyBar.Key.CONTROL -> "ctrl" to "Control"
    KeyBar.Key.LEFT -> "←" to "Left"
    KeyBar.Key.DOWN -> "↓" to "Down"
    KeyBar.Key.UP -> "↑" to "Up"
    KeyBar.Key.RIGHT -> "→" to "Right"
    KeyBar.Key.INTERRUPT -> "^C" to "Control C"
}

private val KEY_FILL = Color(0xFF1A1E25)
private const val KEY_HEIGHT = 48
