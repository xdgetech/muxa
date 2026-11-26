package tech.xdge.muxa.app.next

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlinx.coroutines.launch
import tech.xdge.muxa.app.ui.DestructiveButton
import tech.xdge.muxa.app.ui.Icons
import tech.xdge.muxa.app.ui.LinkButton
import tech.xdge.muxa.app.ui.Palette
import tech.xdge.muxa.app.ui.PrimaryButton
import tech.xdge.muxa.app.ui.Theme
import tech.xdge.muxa.v2.Paused
import tech.xdge.muxa.v2.Terminal
import java.time.Instant
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import java.time.format.FormatStyle

/** What a Task row's long press offers (ADR 0102): Pause Task while it runs, Resume Task while it is paused. */
class RowMenu(val paused: Boolean, val pause: () -> Unit, val resume: () -> Unit)

/** The row menu of [t] in [groups]; null for a terminal that is not a Task's agent. [resumed] gets the new agent. */
fun NextModel.rowMenu(t: Terminal, groups: TerminalGroups, resumed: (Terminal) -> Unit): RowMenu? {
    if (!t.isTaskAgent) return null
    return RowMenu(t.id in groups.paused, { pauseTask(t.id) }, { scope.launch { resumePaused(t.id)?.let(resumed) } })
}

@Composable
fun TaskRowMenu(menu: RowMenu, open: Boolean, dismiss: () -> Unit) {
    DropdownMenu(open, dismiss) {
        if (menu.paused) {
            DropdownMenuItem({ Text("Resume Task") }, { dismiss(); menu.resume() }, Modifier.testTag("task.menu.resume"))
        } else {
            DropdownMenuItem({ Text("Pause Task") }, { dismiss(); menu.pause() }, Modifier.testTag("task.menu.pause"))
        }
    }
}

/**
 * Focus on a paused Task: no terminal runs, so it offers Resume and Close Task.
 * [select] moves Focus to the resumed agent, or away after Close. [back] is the phone's way back.
 */
@Composable
fun PausedView(model: NextModel, paused: Paused, modifier: Modifier, select: (String?) -> Unit, back: (() -> Unit)?) {
    val t = paused.terminal
    val scope = rememberCoroutineScope()
    var busy by remember { mutableStateOf(false) }
    var confirm by remember { mutableStateOf(false) }
    var loss by remember { mutableStateOf<String?>(null) }
    fun run(body: suspend () -> Unit) {
        if (busy) return
        scope.launch {
            busy = true
            try {
                body()
            } finally {
                busy = false
            }
        }
    }
    fun resume() = run { model.resumePaused(t.id)?.let { select(it.id) } }
    fun close(force: Boolean) = run {
        confirm = false
        loss = model.closeTask(t.id, force)
        if (loss == null && model.requestError == null) select(null)
    }

    Column(modifier.testTag("paused.view")) {
        back?.let { IconButton(it, Modifier.testTag("focus.back")) { Icon(Icons.backChevron, "Back", tint = Palette.accent) } }
        Box(Modifier.fillMaxSize().padding(24.dp), contentAlignment = Alignment.Center) {
            Column(
                Modifier.widthIn(max = 420.dp),
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                Text(t.path(), fontSize = 13.sp, color = Palette.textSecondary, textAlign = TextAlign.Center)
                Text(t.title, Modifier.testTag("paused.title"), fontSize = 19.sp, fontWeight = FontWeight.SemiBold, textAlign = TextAlign.Center)
                Text("Paused ${time(paused.pausedAt)}", fontSize = 13.sp, color = Palette.textTertiary)
                Text(
                    "The agent and its shells stopped. The places and the open items stay.",
                    fontSize = 14.sp, color = Palette.textSecondary, textAlign = TextAlign.Center,
                )
                model.requestError?.let { Text(it, Modifier.testTag("paused.error"), color = Theme.failure, fontSize = 13.sp, textAlign = TextAlign.Center) }
                Row(Modifier.padding(top = 8.dp), horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                    PrimaryButton("Resume", ::resume, Modifier.testTag("paused.resume"), enabled = !busy)
                    DestructiveButton("Close Task", { confirm = true }, Modifier.testTag("paused.close"), enabled = !busy)
                }
            }
        }
    }
    if (confirm) {
        AlertDialog(
            onDismissRequest = { confirm = false },
            title = { Text("Close ${t.title}?") },
            text = { Text("Its folder or worktree is removed, and its open Inbox items are marked Done.") },
            confirmButton = { DestructiveButton("Close Task", { close(force = false) }, Modifier.testTag("paused.close.confirm")) },
            dismissButton = { LinkButton("Cancel", { confirm = false }) },
        )
    }
    loss?.let { message ->
        AlertDialog(
            onDismissRequest = { loss = null },
            title = { Text("Remove the worktrees anyway?") },
            text = { Text(message, Modifier.fillMaxWidth().testTag("paused.loss")) },
            confirmButton = { DestructiveButton("Remove Anyway", { close(force = true) }, Modifier.testTag("paused.force")) },
            dismissButton = { LinkButton("Cancel", { loss = null }) },
        )
    }
}

private fun time(unix: Long): String =
    DateTimeFormatter.ofLocalizedDateTime(FormatStyle.MEDIUM, FormatStyle.SHORT).format(Instant.ofEpochSecond(unix).atZone(ZoneId.systemDefault()))
