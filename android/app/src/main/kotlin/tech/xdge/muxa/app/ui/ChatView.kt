package tech.xdge.muxa.app.ui

import androidx.compose.animation.animateContentSize
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TextField
import androidx.compose.material3.TextFieldDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.isShiftPressed
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.mikepenz.markdown.m3.Markdown
import tech.xdge.muxa.app.AppModel
import tech.xdge.muxa.protocol.AgentState
import tech.xdge.muxa.protocol.ChatEntry
import tech.xdge.muxa.protocol.ChatPrompt
import tech.xdge.muxa.protocol.ChatTool
import tech.xdge.muxa.session.ChatLog

/**
 * The Pretty view of an agent pane: the conversation as messages and a composer.
 * Everything sent still goes to the same pane, as a paste and Enter.
 */
@Composable
fun ChatView(model: AppModel, log: ChatLog?) {
    // The composer and the cards keep the messages' readable width, centered.
    Column(
        // Above the keyboard when it shows, else above the navigation bar (the app draws edge to edge).
        Modifier.fillMaxSize().background(Theme.chatBackground).navigationBarsPadding().imePadding().testTag("chat"),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Box(Modifier.weight(1f).fillMaxWidth()) { Messages(log) }
        val waiting = log?.status?.state == AgentState.WAITING
        if (log != null && waiting) {
            val prompt = log.prompt
            if (prompt != null) {
                ApprovalCard(model, prompt, log.entries.lastOrNull { it.id == prompt.entry }?.tool)
            } else {
                WaitingBanner(model)
            }
        }
        log?.ended?.let { EndedBanner(model, it) }
        Composer(model, waiting)
    }
}

@Composable
private fun Messages(log: ChatLog?) {
    if (log == null || !log.loaded) {
        Column(Modifier.fillMaxSize(), verticalArrangement = Arrangement.Center, horizontalAlignment = Alignment.CenterHorizontally) {
            CircularProgressIndicator()
            Text("Loading the conversation…", Modifier.padding(top = 12.dp))
        }
        return
    }
    val list = rememberLazyListState()
    // Reversed, so the newest message is item 0 and the list stays at the bottom when the
    // keyboard shrinks it, as iOS anchors the chat at the bottom. A new last message
    // scrolls into view, as on iOS.
    LaunchedEffect(log.entries.lastOrNull()) { list.animateScrollToItem(0) }
    LazyColumn(
        Modifier.fillMaxSize().testTag("messages"),
        state = list,
        reverseLayout = true,
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(18.dp),
        contentPadding = androidx.compose.foundation.layout.PaddingValues(horizontal = 20.dp, vertical = 16.dp),
    ) {
        items(log.entries.asReversed(), key = { it.id }) { entry ->
            // A readable line length, like the agents' own apps.
            Box(Modifier.widthIn(max = 720.dp).fillMaxWidth()) { EntryRow(entry) }
        }
        if (log.entries.isEmpty()) {
            item { Text("No Messages Yet. Send a prompt to start.", textAlign = TextAlign.Center) }
        }
        if (log.truncated) {
            item { Text("Earlier messages are not shown.", style = MaterialTheme.typography.labelSmall) }
        }
    }
}

@Composable
private fun EntryRow(entry: ChatEntry) {
    when (entry.kind) {
        ChatEntry.Kind.USER -> Row(Modifier.fillMaxWidth()) {
            Spacer(Modifier.weight(1f).widthIn(min = 60.dp))
            SelectionContainer {
                Text(
                    entry.text,
                    Modifier.background(Theme.userBubble, RoundedCornerShape(20.dp)).padding(horizontal = 16.dp, vertical = 10.dp),
                )
            }
        }
        ChatEntry.Kind.ASSISTANT -> SelectionContainer { Markdown(entry.text, modifier = Modifier.fillMaxWidth()) }
        ChatEntry.Kind.TOOL -> entry.tool?.let { ToolRow(it) }
        ChatEntry.Kind.NOTICE -> Text(
            entry.text,
            Modifier.fillMaxWidth(),
            style = MaterialTheme.typography.labelSmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            textAlign = TextAlign.Center,
        )
    }
}

/** A tool call: its name and what it worked on; the result unfolds. */
@Composable
private fun ToolRow(tool: ChatTool) {
    var open by remember { mutableStateOf(false) }
    Column(
        Modifier.fillMaxWidth().background(Theme.toolCard, RoundedCornerShape(12.dp)).animateContentSize()
            .clickable(enabled = tool.result != null) { open = !open }
            .padding(horizontal = 12.dp, vertical = 8.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Text(tool.name, fontWeight = FontWeight.SemiBold, color = if (tool.error) Theme.failure else Color.Unspecified)
            Text(
                tool.summary,
                Modifier.weight(1f),
                fontFamily = FontFamily.Monospace,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                maxLines = 1,
                overflow = TextOverflow.MiddleEllipsis,
            )
            if (tool.result == null) CircularProgressIndicator(Modifier.size(14.dp), strokeWidth = 2.dp) else Text(if (open) "▾" else "▸")
        }
        val result = tool.result
        if (open && result != null) {
            SelectionContainer {
                Text(
                    result.ifEmpty { "(no output)" },
                    Modifier.padding(top = 6.dp),
                    fontFamily = FontFamily.Monospace,
                    style = MaterialTheme.typography.bodySmall,
                    color = if (tool.error) Theme.failure else MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
        }
    }
}

/** The agent waits on something its adapter cannot name: only its terminal shows it. */
@Composable
private fun WaitingBanner(model: AppModel) {
    Row(
        Modifier.fillMaxWidth().background(Theme.attention.copy(alpha = 0.12f)).padding(horizontal = 16.dp, vertical = 8.dp).testTag("waiting"),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        Icon(Icons.waiting, null, tint = Theme.attention)
        Text("Waiting for your answer", Modifier.weight(1f), color = Theme.attention, fontWeight = FontWeight.Medium)
        for (key in listOf("1", "2", "3")) QuickKey(model, key, listOf(key))
        QuickKey(model, "Esc", listOf("Escape"))
        OutlinedButton({ model.setPretty(false) }) { Text("Show Raw") }
    }
}

@Composable
private fun EndedBanner(model: AppModel, reason: String) {
    Row(Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 8.dp), verticalAlignment = Alignment.CenterVertically) {
        Text("Stopped following: $reason", Modifier.weight(1f), color = MaterialTheme.colorScheme.onSurfaceVariant)
        TextButton({ model.retryChat() }) { Text("Retry") }
        TextButton({ model.setPretty(false) }) { Text("Show Raw") }
    }
}

@Composable
private fun QuickKey(model: AppModel, title: String, keys: List<String>) {
    OutlinedButton(
        { model.session.send(keys) },
        Modifier.testTag("quick-${keys.joinToString("")}"),
        contentPadding = androidx.compose.foundation.layout.PaddingValues(horizontal = 10.dp, vertical = 0.dp),
    ) { Text(title, fontFamily = FontFamily.Monospace, fontSize = 13.sp) }
}

/** Enter sends; Shift+Enter starts a new line. */
@Composable
private fun Composer(model: AppModel, waiting: Boolean) {
    var text by remember { mutableStateOf("") }
    val focus = remember { FocusRequester() }
    val hardware = LocalConfiguration.current.hardKeyboardHidden == android.content.res.Configuration.HARDKEYBOARDHIDDEN_NO
    // While the agent waits on a prompt, Enter would answer it: send nothing then.
    val canSend = !waiting && text.isNotBlank()
    fun send() {
        if (!canSend) return
        model.sendPrompt(text)
        text = ""
    }
    LaunchedEffect(model.focusRequest) { if (hardware) runCatching { focus.requestFocus() } }
    Surface(
        Modifier.widthIn(max = 720.dp).fillMaxWidth().padding(horizontal = 16.dp, vertical = 10.dp),
        shape = RoundedCornerShape(22.dp),
        color = Theme.composer,
        tonalElevation = 2.dp,
        shadowElevation = 2.dp,
    ) {
        Column(Modifier.padding(horizontal = 6.dp, vertical = 6.dp)) {
            TextField(
                text,
                { text = it },
                Modifier.fillMaxWidth().focusRequester(focus).testTag("composer").onPreviewKeyEvent { e ->
                    if (e.type == KeyEventType.KeyDown && e.key == Key.Enter && !e.isShiftPressed) {
                        send()
                        true
                    } else {
                        false
                    }
                },
                placeholder = { Text("Message the agent") },
                maxLines = 8,
                // Prompts carry slash commands, paths and code that a phone keyboard would "fix".
                keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.None, autoCorrectEnabled = false),
                colors = TextFieldDefaults.colors(
                    focusedContainerColor = Color.Transparent,
                    unfocusedContainerColor = Color.Transparent,
                    focusedIndicatorColor = Color.Transparent,
                    unfocusedIndicatorColor = Color.Transparent,
                ),
            )
            Row(verticalAlignment = Alignment.CenterVertically) {
                Row(Modifier.weight(1f).horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                    QuickKey(model, "Esc", listOf("Escape"))
                    QuickKey(model, "⏎", listOf("Enter"))
                    QuickKey(model, "⇧Tab", listOf("BTab"))
                    QuickKey(model, "↑", listOf("Up"))
                    QuickKey(model, "↓", listOf("Down"))
                    QuickKey(model, "^C", listOf("C-c"))
                }
                IconButton(::send, enabled = canSend, modifier = Modifier.testTag("send")) { Icon(Icons.send, "Send") }
            }
        }
    }
}

/**
 * What the agent waits on, with the choices its adapter knows. Choices the adapter
 * cannot name stay in the terminal, one tap away.
 */
@Composable
private fun ApprovalCard(model: AppModel, prompt: ChatPrompt, tool: ChatTool?) {
    // The prompt last answered here, so one tap sends its keys once.
    var answered by remember { mutableStateOf<String?>(null) }
    val title = when (prompt.kind) {
        ChatPrompt.Kind.PERMISSION -> "Allow ${tool?.name ?: "this tool"}?"
        ChatPrompt.Kind.QUESTION -> prompt.question ?: "The agent asks a question"
        ChatPrompt.Kind.PLAN -> "The plan is ready"
        else -> "Waiting for your answer"
    }
    val detail = if (prompt.kind == ChatPrompt.Kind.PERMISSION) tool?.summary?.takeIf { it.isNotEmpty() } else null
    // The menu has more than the card offers; it is in the terminal.
    val more = when (prompt.kind) {
        ChatPrompt.Kind.PLAN -> "Approve…"
        ChatPrompt.Kind.QUESTION -> "Other Answer…"
        else -> "More…"
    }
    Column(
        Modifier.widthIn(max = 720.dp).fillMaxWidth().padding(horizontal = 16.dp, vertical = 8.dp)
            .background(Theme.attention.copy(alpha = 0.12f), RoundedCornerShape(16.dp)).padding(14.dp).testTag("approval"),
        verticalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Icon(Icons.waiting, null, tint = Theme.attention)
            Text(title, Modifier.padding(start = 8.dp), color = Theme.attention, fontWeight = FontWeight.SemiBold)
        }
        detail?.let {
            SelectionContainer {
                Text(
                    it,
                    Modifier.fillMaxWidth().background(Theme.toolCard, RoundedCornerShape(10.dp)).padding(10.dp),
                    fontFamily = FontFamily.Monospace,
                    maxLines = 4,
                    overflow = TextOverflow.MiddleEllipsis,
                )
            }
        }
        Row(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            if (prompt.kind == ChatPrompt.Kind.PLAN) {
                Button(model::openPlan, Modifier.testTag("approval-read-plan")) { Text("Read Plan") }
            }
            for (option in prompt.options) {
                val pick = {
                    answered = prompt.entry
                    model.session.send(option.keys)
                }
                if (option.cancel) {
                    OutlinedButton(pick, Modifier.testTag("approval-${option.label}"), enabled = answered != prompt.entry,
                        colors = ButtonDefaults.outlinedButtonColors(contentColor = Theme.failure)) { Text(option.label) }
                } else {
                    Button(pick, Modifier.testTag("approval-${option.label}"), enabled = answered != prompt.entry) { Text(option.label) }
                }
            }
            OutlinedButton({ model.setPretty(false) }, Modifier.testTag("approval-terminal")) { Text(more) }
        }
    }
}
