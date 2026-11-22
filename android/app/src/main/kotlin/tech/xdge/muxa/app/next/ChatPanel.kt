package tech.xdge.muxa.app.next

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import tech.xdge.muxa.app.ui.Icons
import tech.xdge.muxa.app.ui.Theme
import tech.xdge.muxa.v2.ChatEntry
import tech.xdge.muxa.v2.ChatPrompt
import tech.xdge.muxa.v2.Terminal

/**
 * Chat beside a terminal (plan F4): the conversation of a Task's agent from its
 * own records, the choice it waits on, and a line to send it. Answers go to the
 * terminal as input; muxa never answers a menu by itself.
 */
@Composable
fun ChatPanel(model: NextModel, terminal: Terminal, modifier: Modifier) {
    var chat by remember(terminal.id) { mutableStateOf<Int?>(null) }
    var attempt by remember(terminal.id) { mutableIntStateOf(0) }
    LaunchedEffect(terminal.id, attempt) { chat = model.openChat(terminal.id) }
    DisposableEffect(terminal.id) { onDispose { chat?.let(model::closeChat) } }
    val n = chat
    val ended = n?.let { model.chatEnded[it] }
    val entries = n?.let { model.chats[it] }
    Column(modifier.testTag("focus.chatpanel")) {
        when {
            ended != null -> Column(Modifier.fillMaxSize().padding(16.dp), verticalArrangement = Arrangement.Center,
                horizontalAlignment = Alignment.CenterHorizontally) {
                Text(ended, color = MaterialTheme.colorScheme.onSurfaceVariant)
                TextButton({
                    n.let(model::closeChat)
                    attempt += 1
                }) { Text("Open again") }
            }
            entries != null -> Conversation(model, terminal.id, entries, model.chatPrompts[n], Modifier.weight(1f))
            else -> Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) { CircularProgressIndicator() }
        }
    }
}

@Composable
private fun Conversation(model: NextModel, terminal: String, entries: List<ChatEntry>, prompt: ChatPrompt?, modifier: Modifier) {
    val list = rememberLazyListState()
    LaunchedEffect(entries.size) { if (entries.isNotEmpty()) list.scrollToItem(entries.size - 1) }
    var draft by rememberSaveable(terminal) { mutableStateOf("") }
    fun sendDraft() {
        val text = draft.trim()
        if (text.isEmpty()) return
        draft = ""
        model.send("$text\r", terminal)
    }
    Column(modifier) {
        LazyColumn(Modifier.weight(1f).fillMaxWidth().testTag("chat.list"), state = list) {
            items(entries, key = { it.id }) { ChatRow(it) }
        }
        prompt?.let { p -> PromptCard(p) { model.send(it, terminal) } }
        Row(Modifier.padding(8.dp), verticalAlignment = Alignment.CenterVertically) {
            OutlinedTextField(draft, { draft = it }, Modifier.weight(1f).testTag("chat.field"),
                placeholder = { Text("Message the agent") }, singleLine = true)
            IconButton(::sendDraft, Modifier.testTag("chat.send"), enabled = draft.isNotBlank()) { Icon(Icons.send, "Send") }
        }
        model.requestError?.let { Text(it, Modifier.padding(horizontal = 8.dp), color = MaterialTheme.colorScheme.error) }
    }
}

@Composable
private fun ChatRow(e: ChatEntry) {
    val pad = Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 6.dp)
    when (e.kind) {
        "user" -> Box(pad, contentAlignment = Alignment.CenterEnd) {
            Text(e.text.orEmpty(), Modifier
                .background(MaterialTheme.colorScheme.secondaryContainer, RoundedCornerShape(8.dp))
                .padding(8.dp)
                .testTag("chat.user"))
        }
        "tool" -> Column(pad.testTag("chat.tool")) {
            Text("${e.tool?.name ?: "Tool"} ${e.tool?.summary.orEmpty()}", style = MaterialTheme.typography.labelMedium,
                fontFamily = FontFamily.Monospace, maxLines = 2, overflow = TextOverflow.Ellipsis)
            e.tool?.result?.let {
                Text(it, style = MaterialTheme.typography.labelSmall, fontFamily = FontFamily.Monospace, maxLines = 4,
                    color = if (e.tool?.error == true) MaterialTheme.colorScheme.error else MaterialTheme.colorScheme.onSurfaceVariant)
            }
        }
        "notice" -> Text(e.text.orEmpty(), pad, style = MaterialTheme.typography.labelSmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant)
        else -> Text(e.text.orEmpty(), pad.testTag("chat.assistant"))
    }
}

/** The choice the agent waits on, as buttons that send the checked input. */
@Composable
private fun PromptCard(p: ChatPrompt, pick: (String) -> Unit) {
    val title = when (p.kind) {
        "permission" -> "The agent asks to go on"
        "question" -> "The agent asks"
        "plan" -> "The agent asks to leave plan mode"
        else -> "The agent waits"
    }
    Column(Modifier.fillMaxWidth().background(Theme.attention.copy(alpha = 0.12f)).padding(10.dp).testTag("chat.prompt"),
        verticalArrangement = Arrangement.spacedBy(6.dp)) {
        Text(title, style = MaterialTheme.typography.titleSmall)
        p.question?.let { Text(it) }
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            for (o in p.options) {
                OutlinedButton({ pick(o.input) }, Modifier.testTag("chat.option.${o.label}")) { Text(o.label) }
            }
        }
    }
}
