package tech.xdge.muxa.app.ui

import androidx.compose.material3.AlertDialog
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import tech.xdge.muxa.app.AppModel
import tech.xdge.muxa.app.loseWork

/**
 * Asks before a Done or Drop removes a worktree whose work could be lost (ADR 0043). The host
 * changed nothing yet: Cancel keeps the Task as it is, and Remove sends the end again with `force`.
 */
@Composable
fun LoseWorkDialog(model: AppModel) {
    val pending = model.losingWork ?: return
    AlertDialog(
        onDismissRequest = { model.losingWork = null },
        title = { Text("Remove the Worktree?") },
        text = { Text(loseWorkMessage(pending.reason), Modifier.testTag("lose-work-message")) },
        confirmButton = {
            TextButton({ model.loseWork() }, Modifier.testTag("lose-work-remove")) {
                Text("Remove", color = MaterialTheme.colorScheme.error)
            }
        },
        dismissButton = { TextButton({ model.losingWork = null }, Modifier.testTag("lose-work-cancel")) { Text("Cancel") } },
    )
}

/** The host's reason, e.g. "its branch is not merged", as the dialog's first sentence. */
internal fun loseWorkMessage(reason: String): String =
    "${reason.trimEnd('.').replaceFirstChar { it.uppercaseChar() }}. Remove its worktree and branch anyway?"
