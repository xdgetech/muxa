package tech.xdge.muxa.app.next

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.launch
import tech.xdge.muxa.v2.LastCheck
import tech.xdge.muxa.v2.LoopInfo
import tech.xdge.muxa.v2.LoopSpec
import tech.xdge.muxa.v2.Terminal

/**
 * The Task Loop of a Task's agent (ADR 0019, plan F3): where it stands, and
 * Start, Start again and Stop. The host runs the check after each finished
 * turn and types the fix prompt; muxa never answers the agent's menus.
 */
@Composable
fun LoopDialog(model: NextModel, terminal: Terminal, close: () -> Unit) {
    val lp = model.loops[terminal.id]
    var check by remember { mutableStateOf(lp?.check.orEmpty()) }
    var attempts by remember { mutableIntStateOf(lp?.maxAttempts ?: 3) }
    val scope = rememberCoroutineScope()
    AlertDialog(
        onDismissRequest = close,
        title = { Text("Task Loop") },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                if (lp != null) {
                    Column(Modifier.testTag("loop.now")) {
                        Text(LoopText.state(lp), style = MaterialTheme.typography.titleSmall)
                        Text("Failed checks: ${lp.attempts} of ${lp.maxAttempts}")
                        lp.lastCheck?.let { Text("Last check: ${LoopText.last(it)}") }
                    }
                }
                OutlinedTextField(
                    check, { check = it }, Modifier.fillMaxWidth().testTag("loop.check"),
                    label = { Text("Check") }, placeholder = { Text("The workspace's check") },
                    singleLine = true, textStyle = MaterialTheme.typography.bodyMedium.copy(fontFamily = FontFamily.Monospace),
                )
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text("Stop after $attempts failed checks", Modifier.weight(1f))
                    TextButton({ attempts = (attempts - 1).coerceAtLeast(1) }) { Text("−") }
                    TextButton({ attempts = (attempts + 1).coerceAtMost(MAX_ATTEMPTS) }) { Text("+") }
                }
                Text(
                    "After each finished turn the host runs the check in the Task's folder. " +
                        "Leave it empty for the workspace's only allowed check.",
                    style = MaterialTheme.typography.bodySmall,
                )
                model.requestError?.let { Text(it, color = MaterialTheme.colorScheme.error, modifier = Modifier.testTag("loop.error")) }
            }
        },
        confirmButton = {
            TextButton({
                scope.launch {
                    val spec = LoopSpec(check = check.trim().ifEmpty { null }, maxAttempts = attempts)
                    model.startLoop(terminal.id, spec, reset = true)
                    if (model.requestError == null) close()
                }
            }, Modifier.testTag("loop.start")) { Text(if (lp == null) "Start" else "Start again") }
        },
        dismissButton = {
            Row {
                if (lp?.isActive == true) TextButton({ model.stopLoop(terminal.id) }, Modifier.testTag("loop.stop")) { Text("Stop") }
                TextButton(close) { Text("Close") }
            }
        },
    )
}

/** How a loop reads in a row or a dialog. */
object LoopText {
    fun state(lp: LoopInfo): String = when {
        lp.state == "armed" -> "Waits for the turn's end"
        lp.state == "checking" -> "Checking"
        lp.state == "paused" -> "Paused: the agent waits for you"
        lp.state == "passed" -> "Passed"
        lp.state == "stopped" && lp.reason == "limit" -> "Stopped: too many failed checks"
        lp.state == "stopped" && lp.reason == "no-agent" -> "Stopped: the agent ended"
        lp.state == "stopped" && lp.reason != null && lp.reason != "user" -> "Stopped: ${lp.reason}"
        lp.state == "stopped" -> "Stopped"
        else -> lp.state
    }

    /** `loop armed 1/3`, short for a list row. */
    fun badge(lp: LoopInfo) = "loop ${lp.state} ${lp.attempts}/${lp.maxAttempts}"

    fun last(c: LastCheck): String {
        val how = c.exit?.let { if (it == 0) "passed" else "failed with $it" } ?: "timed out"
        return "#${c.attempt} $how in ${c.seconds} s"
    }
}

private const val MAX_ATTEMPTS = 10
