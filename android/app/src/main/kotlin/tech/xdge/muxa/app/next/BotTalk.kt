package tech.xdge.muxa.app.next

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.launch
import tech.xdge.muxa.app.ui.LinkButton
import tech.xdge.muxa.app.ui.PrimaryButton
import tech.xdge.muxa.app.ui.Theme
import tech.xdge.muxa.v2.Terminal

/** Starts a talk with a Bot of [workspace] (ADR 0083), a Task like any other, and opens it with [open]. */
class BotTalk(private val model: NextModel, val workspace: String, private val open: (Terminal) -> Unit) {
    /** Null when the talk started and opened; else why it did not. */
    suspend fun start(bot: String): String? {
        val started = model.startTask(TaskDraft.talk(workspace, bot)) ?: return model.requestError ?: NOT_STARTED
        open(started)
        return null
    }

    private companion object {
        const val NOT_STARTED = "The talk did not start."
    }
}

/** Asks for a new Bot's name, then starts a talk that may propose adding it. */
@Composable
fun TalkSetupDialog(talk: BotTalk, close: () -> Unit) {
    var name by remember { mutableStateOf("") }
    var error by remember { mutableStateOf<String?>(null) }
    var busy by remember { mutableStateOf(false) }
    val scope = rememberCoroutineScope()
    val valid = BotDraft.isValidName(name.trim())
    AlertDialog(
        onDismissRequest = close,
        modifier = Modifier.testTag("bot.setup"),
        title = { Text("Set Up a Bot") },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Field(name, { name = it }, "Name", "bot.setup.name", placeholder = "issue-triage", help = BotDraft.NAME_RULE)
                Text(
                    "The Bot talks with you in a Task of ${talk.workspace}. When you agree, it proposes its settings as an Inbox item.",
                    style = MaterialTheme.typography.bodySmall,
                )
                error?.let { Text(it, Modifier.testTag("bot.setup.error"), color = Theme.failure) }
            }
        },
        confirmButton = {
            PrimaryButton("Start Talk", {
                scope.launch {
                    busy = true
                    error = talk.start(name.trim())
                    busy = false
                    if (error == null) close()
                }
            }, Modifier.testTag("bot.setup.start"), enabled = valid && !busy)
        },
        dismissButton = { LinkButton("Cancel", close) },
    )
}
