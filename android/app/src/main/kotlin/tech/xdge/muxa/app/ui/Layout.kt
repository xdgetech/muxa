package tech.xdge.muxa.app.ui

import androidx.compose.runtime.compositionLocalOf

/**
 * The phone layout: compact in either direction, never decided by the device (ADR 0012,
 * 0013). A phone in landscape is compact tall; a folded foldable is compact, an unfolded
 * one is not. Window size class breakpoints: Compact width < 600 dp, Compact height < 480 dp.
 */
fun isPhoneLayout(widthDp: Int, heightDp: Int): Boolean = widthDp < 600 || heightDp < 480

/** A hinge as Jetpack WindowManager reports it, in the window's dp. */
data class Hinge(
    val vertical: Boolean,
    val halfOpened: Boolean,
    /** It splits the window in two: a hinge with a gap, or a half-open fold. */
    val separating: Boolean,
    /** Its start and end along the axis it splits: x for a vertical hinge, y for a horizontal one. */
    val start: Int,
    val end: Int,
)

/** How the window is laid out (ADR 0013's table). */
sealed interface WindowLayout {
    /** A stack: the list, or the pane pushed over it. */
    data object Phone : WindowLayout

    /** The list beside the pane, hidden once a pane is open. */
    data object Tablet : WindowLayout

    /** Half-open like a book: the list left of the hinge, the pane right of it, both shown. */
    data class Book(val listWidth: Int, val gap: Int) : WindowLayout

    /** Half-open on a table: the app above the hinge, the keys below it. */
    data class Tabletop(val topHeight: Int, val gap: Int) : WindowLayout
}

/** The layout for a window of [widthDp] x [heightDp] with an optional [hinge]. */
fun windowLayout(widthDp: Int, heightDp: Int, hinge: Hinge?): WindowLayout {
    // The grid avoids the hinge only when it separates or the device is half open.
    if (hinge != null && (hinge.separating || hinge.halfOpened)) {
        if (hinge.vertical) return WindowLayout.Book(hinge.start, hinge.end - hinge.start)
        if (hinge.halfOpened) return WindowLayout.Tabletop(hinge.start, hinge.end - hinge.start)
    }
    return if (isPhoneLayout(widthDp, heightDp)) WindowLayout.Phone else WindowLayout.Tablet
}

/** Set at the root from the window's layout, so every screen reads the same answer. */
val LocalPhoneLayout = compositionLocalOf { false }
