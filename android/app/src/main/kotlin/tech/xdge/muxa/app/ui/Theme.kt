package tech.xdge.muxa.app.ui

import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Shapes
import androidx.compose.material3.darkColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color

/** The design's dark palette: every surface, line and text color the app draws. */
object Palette {
    val ground = Color(0xFF0E1014)
    val surface = Color(0xFF14171C)
    val raised = Color(0xFF161A20)
    /** Buttons and fields that sit on a surface. */
    val control = Color(0xFF1E232B)
    /** The picked segment or row. */
    val picked = Color(0xFF2A2F38)
    val border = Color(0xFF262B33)
    val borderStrong = Color(0xFF2C323C)
    /** The line between rows inside a card. */
    val divider = Color(0xFF20252D)
    val text = Color(0xFFE7E9EC)
    val textSecondary = Color(0xFFA3ABB6)
    val textTertiary = Color(0xFF8B94A0)
    val accent = Color(0xFF6AA6FF)
    val terminal = Color(0xFF0A0C0F)

    /** A note that explains, such as Settings' line about secrets. */
    val infoBackground = Color(0xFF16202E)
    val infoBorder = Color(0xFF24344A)
    val infoText = Color(0xFFC9D8EE)

    /** A secondary button's fill and border. */
    val secondaryFill = Color(0xFF232831)
    val secondaryBorder = Color(0xFF323843)

    /** Delete and Remove: a muted red on a dark red fill, as the design's failed chip. */
    val destructive = Color(0xFFFF9B9B)
    val destructiveFill = Color(0xFF3A1717)
    val destructiveBorder = Color(0xFF5A2626)
}

/** A Task's state as its program reports it (OSC 7501), by color. */
object StateColors {
    val needsYou = Color(0xFFF5A524)
    val done = Color(0xFF6AA6FF)
    val working = Color(0xFF3FC79A)
    val idle = Color(0xFF8B94A0)

    /** The amber banner behind Needs you. */
    val needsYouBackground = Color(0xFF2A2110)
    val needsYouBorder = Color(0xFF5A4416)
    /** Text on the amber button. */
    val onNeedsYou = Color(0xFF1A1203)
}

/** The colors of an Inbox kind's chip: its background and its text. */
data class KindColors(val background: Color, val content: Color)

object KindPalette {
    val review = KindColors(Color(0xFF123326), Color(0xFF7FE0B8))
    val failed = KindColors(Color(0xFF3A1717), Color(0xFFFF9B9B))
    val reply = KindColors(Color(0xFF2A2110), Color(0xFFF5C26B))
    val decide = KindColors(Color(0xFF251D38), Color(0xFFCDB6FF))
    val read = KindColors(Color(0xFF14263F), Color(0xFF9CC4FF))
    val todo = KindColors(Color(0xFF232831), Color(0xFFC9CED6))
}

/**
 * The app's colors by meaning, as on iOS (`Theme.swift`). Views use these names
 * or the palettes above. Besides the accent, only states and kinds have a color.
 */
object Theme {
    val accent = Palette.accent

    /** Behind the pane: the terminal's own background. */
    val terminal = Palette.terminal
    val connected = StateColors.working
    val attention = StateColors.needsYou
    val failure = Color(0xFFFF9B9B)

    /** ✓ closes an item (ADR 0040). */
    val done = Color(0xFF2E8B6A)

    /** Later moves an item to the end of the queue. */
    val later = Color(0xFF5B4BB8)

    /** Lines a change adds and removes. */
    val added = StateColors.working
    val removed = Color(0xFFFF6B6B)

    val chatBackground = Palette.ground
    val userBubble = Palette.picked
    val toolCard = Palette.raised
    val composer = Palette.surface
}

private val scheme = darkColorScheme(
    primary = Palette.accent,
    onPrimary = Palette.ground,
    primaryContainer = Color(0xFF14263F),
    onPrimaryContainer = Color(0xFF9CC4FF),
    secondary = Palette.textSecondary,
    onSecondary = Palette.ground,
    secondaryContainer = Palette.picked,
    onSecondaryContainer = Palette.text,
    tertiary = StateColors.needsYou,
    onTertiary = StateColors.onNeedsYou,
    background = Palette.ground,
    onBackground = Palette.text,
    surface = Palette.ground,
    onSurface = Palette.text,
    surfaceVariant = Palette.raised,
    onSurfaceVariant = Palette.textSecondary,
    surfaceContainerLowest = Palette.ground,
    surfaceContainerLow = Palette.surface,
    surfaceContainer = Palette.surface,
    surfaceContainerHigh = Palette.raised,
    surfaceContainerHighest = Palette.control,
    surfaceBright = Palette.control,
    surfaceDim = Palette.ground,
    inverseSurface = Palette.text,
    inverseOnSurface = Palette.ground,
    outline = Palette.borderStrong,
    outlineVariant = Palette.border,
    error = Theme.failure,
    onError = Palette.ground,
    errorContainer = KindPalette.failed.background,
    onErrorContainer = KindPalette.failed.content,
    scrim = Color.Black,
)

/** Dialogs and menus as the design's 16 dp cards, not Material's 28 dp. */
private val shapes = Shapes(extraLarge = Corners.dialog)

/** The design is dark only: the app wears it whatever the system's setting. */
@Composable
fun MuxaTheme(content: @Composable () -> Unit) {
    MaterialTheme(colorScheme = scheme, shapes = shapes, content = content)
}
