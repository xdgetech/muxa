package tech.xdge.muxa.app.ui

import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.ReadOnlyComposable
import androidx.compose.ui.graphics.Color
import tech.xdge.muxa.protocol.AgentState

/**
 * The app's colors by meaning, as on iOS (`Theme.swift`). Views use these names only.
 * Besides the accent, only states have a color.
 */
object Theme {
    val accent = Color(0xFF0A84FF)

    /** Behind the pane: the terminal's own background. */
    val terminal = Color.Black
    val connected = Color(0xFF34C759)
    val attention = Color(0xFFFF9500)
    val failure = Color(0xFFFF3B30)

    /** Lines a change adds and removes. */
    val added = Color(0xFF34C759)
    val removed = Color(0xFFFF3B30)

    fun state(s: AgentState): Color = when (s) {
        AgentState.RUNNING -> Color(0xFF0A84FF)
        AgentState.WAITING -> attention
        AgentState.IDLE -> Color(0xFF8E8E93)
        AgentState.DONE -> connected
        AgentState.ERROR -> failure
    }

    val chatBackground: Color
        @Composable @ReadOnlyComposable get() = MaterialTheme.colorScheme.background
    val userBubble: Color
        @Composable @ReadOnlyComposable get() = MaterialTheme.colorScheme.secondaryContainer
    val toolCard: Color
        @Composable @ReadOnlyComposable get() = MaterialTheme.colorScheme.surfaceVariant
    val composer: Color
        @Composable @ReadOnlyComposable get() = MaterialTheme.colorScheme.surface
}

@Composable
fun MuxaTheme(content: @Composable () -> Unit) {
    val scheme = if (isSystemInDarkTheme()) {
        darkColorScheme(primary = Theme.accent)
    } else {
        lightColorScheme(primary = Theme.accent)
    }
    MaterialTheme(colorScheme = scheme, content = content)
}
