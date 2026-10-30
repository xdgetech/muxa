package tech.xdge.muxa.app.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import tech.xdge.muxa.protocol.AgentState
import tech.xdge.muxa.protocol.Pane
import tech.xdge.muxa.session.shown

/** The agent's state and its age, only when an adapter reported it; else the command. */
@Composable
fun AgentBadge(pane: Pane) {
    val agent = pane.agent
    val style = MaterialTheme.typography.labelSmall
    val muted = MaterialTheme.colorScheme.onSurfaceVariant
    if (agent == null) {
        Text(pane.command, style = style, color = muted, fontFamily = FontFamily.Monospace)
        return
    }
    val state = agent.shown(pane.shellForeground)
    if (state == null) {
        Text("unknown (stale)", style = style, color = muted)
        return
    }
    Row(verticalAlignment = Alignment.CenterVertically) {
        Box(Modifier.size(8.dp).background(Theme.state(state), CircleShape))
        Text(name(state), Modifier.padding(start = 4.dp), style = style)
        agent.since?.let { Text(" " + AgentSummary.age(it), style = style, color = muted) }
    }
}

private fun name(s: AgentState) = s.name.lowercase()

/** Words for a pane's state, as the iOS `AgentBadge` writes them. */
object AgentSummary {
    /** Epoch seconds to "5s", "5m", "2h", "3d". */
    fun age(since: Long, now: Long = System.currentTimeMillis() / 1000): String {
        val s = (now - since).coerceAtLeast(0)
        return when {
            s < 60 -> "${s}s"
            s < 3600 -> "${s / 60}m"
            s < 86400 -> "${s / 3600}h"
            else -> "${s / 86400}d"
        }
    }

    /** One line for a title: "waiting 1m · 100×30", or the command when no adapter reports. */
    fun summary(pane: Pane): String {
        val size = "${pane.cols}×${pane.rows}"
        val agent = pane.agent ?: return "${pane.command} · $size"
        val state = agent.shown(pane.shellForeground) ?: return "unknown (stale) · $size"
        return name(state) + (agent.since?.let { " " + age(it) } ?: "") + " · " + size
    }
}
