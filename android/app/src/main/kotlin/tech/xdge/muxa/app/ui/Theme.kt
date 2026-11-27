package tech.xdge.muxa.app.ui

import androidx.compose.material3.ColorScheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Shapes
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.graphics.Color
import tech.xdge.muxa.session.ThemeToken

private fun token(t: ThemeToken): Color = ThemeStore.current[t]

/**
 * Every surface, line and text color the app draws, from the app's theme (ADR 0091).
 * Each is a getter, so composition that reads one recomposes on a new pick.
 */
object Palette {
    val ground: Color get() = token(ThemeToken.ground)
    val surface: Color get() = token(ThemeToken.surface)
    val raised: Color get() = token(ThemeToken.card)
    /** Buttons and fields that sit on a surface. */
    val control: Color get() = token(ThemeToken.raised)
    /** The terminal's key bar: between [raised] and [control]. */
    val cardRaised: Color get() = token(ThemeToken.cardRaised)
    /** The picked segment or row. */
    val picked: Color get() = token(ThemeToken.selected)
    val border: Color get() = token(ThemeToken.border)
    val borderStrong: Color get() = token(ThemeToken.borderStrong)
    /** The line between rows inside a card. */
    val divider: Color get() = token(ThemeToken.divider)
    val text: Color get() = token(ThemeToken.text)
    val textSecondary: Color get() = token(ThemeToken.textSecondary)
    val textTertiary: Color get() = token(ThemeToken.textTertiary)
    val accent: Color get() = token(ThemeToken.accent)
    val terminal: Color get() = token(ThemeToken.terminalBackground)

    /** A note that explains, such as Settings' line about secrets. */
    val infoBackground: Color get() = token(ThemeToken.infoBack)
    val infoBorder: Color get() = token(ThemeToken.infoBorder)
    val infoText: Color get() = token(ThemeToken.infoText)

    /** A secondary button's fill and border. */
    val secondaryFill: Color get() = token(ThemeToken.rowSelected)
    val secondaryBorder: Color get() = token(ThemeToken.buttonBorder)

    /** Delete and Remove: a muted red on a dark red fill, as the design's failed chip. */
    val destructive: Color get() = token(ThemeToken.failure)
    val destructiveFill: Color get() = token(ThemeToken.destructiveBack)
    val destructiveBorder: Color get() = token(ThemeToken.destructiveBorder)

    /** A value past the host's limits, such as memory under pressure. */
    val warnBack: Color get() = token(ThemeToken.warnBack)
    val warnBorder: Color get() = token(ThemeToken.warnBorder)
    val warnText: Color get() = token(ThemeToken.warnText)
}

/** A Task's state as its program reports it (OSC 7501), by color. */
object StateColors {
    val needsYou: Color get() = token(ThemeToken.needsYou)
    val done: Color get() = token(ThemeToken.done)
    val working: Color get() = token(ThemeToken.working)
    val idle: Color get() = token(ThemeToken.idle)

    /** The amber banner behind Needs you. */
    val needsYouBackground: Color get() = token(ThemeToken.promptBack)
    val needsYouBorder: Color get() = token(ThemeToken.promptBorder)
    /** Text on the amber button. */
    val onNeedsYou: Color get() = token(ThemeToken.onNeedsYou)
}

/** The colors of an Inbox kind's chip: its background and its text. */
data class KindColors(val background: Color, val content: Color)

object KindPalette {
    val review: KindColors get() = kind(ThemeToken.okBack, ThemeToken.okText)
    val failed: KindColors get() = kind(ThemeToken.destructiveBack, ThemeToken.failedText)
    val reply: KindColors get() = kind(ThemeToken.writeBack, ThemeToken.writeText)
    val decide: KindColors get() = kind(ThemeToken.decideBack, ThemeToken.decide)
    val read: KindColors get() = kind(ThemeToken.readBack, ThemeToken.readText)
    val todo: KindColors get() = kind(ThemeToken.todoBack, ThemeToken.todoText)

    private fun kind(background: ThemeToken, content: ThemeToken) = KindColors(token(background), token(content))
}

/**
 * The app's colors by meaning, as on iOS (`Theme.swift`). Views use these names
 * or the palettes above. Besides the accent, only states and kinds have a color.
 */
object Theme {
    val accent: Color get() = Palette.accent

    /** Behind the pane: the terminal's own background. */
    val terminal: Color get() = Palette.terminal
    val connected: Color get() = StateColors.working
    val attention: Color get() = StateColors.needsYou
    val failure: Color get() = token(ThemeToken.failure)

    /** ✓ closes an item (ADR 0040). */
    val done: Color get() = token(ThemeToken.swipeDone)

    /** Later moves an item to the end of the queue. */
    val later: Color get() = token(ThemeToken.swipeLater)

    /** Lines a change adds and removes. */
    val added: Color get() = token(ThemeToken.diffAdded)
    val removed: Color get() = token(ThemeToken.diffRemoved)

    val chatBackground: Color get() = Palette.ground
    val userBubble: Color get() = token(ThemeToken.userBubble)
    val toolCard: Color get() = Palette.raised
    val composer: Color get() = Palette.surface
}

/** Material's roles from the tokens, so its own components wear the theme too. */
private fun colorScheme(t: AppTheme): ColorScheme {
    fun c(token: ThemeToken) = t[token]
    val ground = c(ThemeToken.ground)
    val text = c(ThemeToken.text)
    val scheme = if (t.isDark) darkColorScheme() else lightColorScheme()
    return scheme.copy(
        primary = c(ThemeToken.accent),
        onPrimary = c(ThemeToken.onAccent),
        primaryContainer = c(ThemeToken.readBack),
        onPrimaryContainer = c(ThemeToken.readText),
        secondary = c(ThemeToken.textSecondary),
        onSecondary = ground,
        secondaryContainer = c(ThemeToken.selected),
        onSecondaryContainer = text,
        tertiary = c(ThemeToken.needsYou),
        onTertiary = c(ThemeToken.onNeedsYou),
        background = ground,
        onBackground = text,
        surface = ground,
        onSurface = text,
        surfaceVariant = c(ThemeToken.card),
        onSurfaceVariant = c(ThemeToken.textSecondary),
        surfaceContainerLowest = ground,
        surfaceContainerLow = c(ThemeToken.surface),
        surfaceContainer = c(ThemeToken.surface),
        surfaceContainerHigh = c(ThemeToken.card),
        surfaceContainerHighest = c(ThemeToken.raised),
        surfaceBright = c(ThemeToken.raised),
        surfaceDim = ground,
        inverseSurface = text,
        inverseOnSurface = ground,
        outline = c(ThemeToken.borderStrong),
        outlineVariant = c(ThemeToken.border),
        error = c(ThemeToken.failure),
        onError = ground,
        errorContainer = c(ThemeToken.destructiveBack),
        onErrorContainer = c(ThemeToken.failedText),
        scrim = Color.Black,
    )
}

/** Dialogs and menus as the design's 16 dp cards, not Material's 28 dp. */
private val shapes = Shapes(extraLarge = Corners.dialog)

/** The device's theme (ADR 0091), whatever the system's setting. */
@Composable
fun MuxaTheme(content: @Composable () -> Unit) {
    val theme = ThemeStore.current
    val scheme = remember(theme) { colorScheme(theme) }
    MaterialTheme(colorScheme = scheme, shapes = shapes, content = content)
}
