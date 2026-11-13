package tech.xdge.muxa.app.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.input.nestedscroll.NestedScrollConnection
import androidx.compose.ui.input.nestedscroll.NestedScrollSource
import androidx.compose.ui.input.nestedscroll.nestedScroll
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.unit.dp
import tech.xdge.muxa.app.AppModel
import tech.xdge.muxa.app.PaneHistory
import tech.xdge.muxa.app.pane.Palette
import tech.xdge.muxa.terminal.Run
import tech.xdge.muxa.terminal.ScreenSnapshot

/** A monospace cell is about this wide, in ems. */
private const val CELL_EMS = 0.6f
private const val MIN_FONT_SP = 6f
private const val MAX_FONT_SP = 16f

/** A drag up past the newest line by this much returns to the live pane, as on iOS. */
private val PULL_PAST_END = 60.dp

/**
 * The shown pane's scrollback in the pane's place, opened at the newest line, so a scroll
 * up on the pane goes on into older lines. Reading it changes nothing on the host.
 */
@Composable
fun ScrollbackLayer(model: AppModel, history: PaneHistory) {
    Box(Modifier.fillMaxSize().background(Color(Palette.BACKGROUND)).testTag("history")) {
        when (history) {
            PaneHistory.Loading -> Centered { CircularProgressIndicator() }
            is PaneHistory.Failed -> Centered { Text(history.reason, color = Color.White) }
            is PaneHistory.Loaded -> Lines(history, onLive = model::closeHistory)
        }
        Button(model::closeHistory, Modifier.align(Alignment.BottomEnd).padding(16.dp).testTag("history-live")) {
            Icon(Icons.live, null)
            Text("Live", Modifier.padding(start = 8.dp))
        }
    }
}

@Composable
private fun Centered(content: @Composable () -> Unit) {
    Box(Modifier.fillMaxSize().padding(32.dp), contentAlignment = Alignment.Center) { content() }
}

@Composable
private fun Lines(history: PaneHistory.Loaded, onLive: () -> Unit) {
    val state = rememberLazyListState(initialFirstVisibleItemIndex = history.lines.lastIndex.coerceAtLeast(0))
    val pull = with(LocalDensity.current) { PULL_PAST_END.toPx() }
    // A drag up that the list cannot take any more: it is at the newest line.
    val pastEnd = remember(onLive) {
        object : NestedScrollConnection {
            var pulled = 0f
            override fun onPostScroll(consumed: Offset, available: Offset, source: NestedScrollSource): Offset {
                if (source != NestedScrollSource.UserInput || available.y >= 0f) {
                    pulled = 0f
                    return Offset.Zero
                }
                pulled -= available.y
                if (pulled > pull) {
                    pulled = Float.NEGATIVE_INFINITY
                    onLive()
                }
                return Offset.Zero
            }
        }
    }
    BoxWithConstraints(Modifier.fillMaxSize().nestedScroll(pastEnd)) {
        val density = LocalDensity.current
        val size = with(density) {
            val px = maxWidth.toPx() / (history.cols.coerceAtLeast(1) * CELL_EMS)
            (px / fontScale / this.density).coerceIn(MIN_FONT_SP, MAX_FONT_SP)
        }
        val style = MaterialTheme.typography.bodySmall.copy(
            fontFamily = FontFamily.Monospace,
            fontSize = androidx.compose.ui.unit.TextUnit(size, androidx.compose.ui.unit.TextUnitType.Sp),
            lineHeight = androidx.compose.ui.unit.TextUnit(size * 1.2f, androidx.compose.ui.unit.TextUnitType.Sp),
            color = Color(Palette.foreground(tech.xdge.muxa.terminal.TermColor.Default, false)),
        )
        LazyColumn(Modifier.fillMaxSize().testTag("history-lines"), state = state) {
            if (history.truncated) {
                item { Text("Older lines are not shown.", Modifier.padding(8.dp), color = Color.Gray) }
            }
            itemsIndexed(history.lines) { _, line ->
                Text(annotated(line), Modifier.fillMaxWidth(), style = style, softWrap = false, maxLines = 1)
            }
        }
    }
}

/** A line's runs with their colors; the gaps between runs are spaces. */
private fun annotated(line: List<Run>): AnnotatedString = buildAnnotatedString {
    var column = 0
    for (run in line) {
        while (column < run.column) {
            append(' ')
            column++
        }
        val s = run.style
        var fg = Palette.foreground(s.foreground, s.bold)
        var bg = Palette.background(s.background)
        if (s.inverse) fg = bg.also { bg = fg }
        if (s.dim) fg = Palette.dim(fg)
        val text = run.text.filter { it != ScreenSnapshot.WIDE_TAIL }.replace('\u0000', ' ')
        val span = SpanStyle(
            color = if (s.hidden) Color.Transparent else Color(fg),
            background = if (bg == Palette.BACKGROUND) Color.Unspecified else Color(bg),
            fontWeight = if (s.bold) FontWeight.Bold else null,
            fontStyle = if (s.italic) FontStyle.Italic else null,
            textDecoration = if (s.underline) TextDecoration.Underline else null,
        )
        pushStyle(span)
        append(text)
        pop()
        column += run.text.length
    }
}
