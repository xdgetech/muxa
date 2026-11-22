package tech.xdge.muxa.app.next


import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Text
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
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import tech.xdge.muxa.app.ui.LinkButton
import tech.xdge.muxa.app.ui.Palette
import tech.xdge.muxa.app.ui.PrimaryButton
import tech.xdge.muxa.app.ui.SecondaryButton
import tech.xdge.muxa.app.ui.StateColors
import tech.xdge.muxa.app.ui.Theme
import tech.xdge.muxa.app.ui.Tone
import tech.xdge.muxa.v2.ChatEntry
import tech.xdge.muxa.v2.ChatPrompt
import tech.xdge.muxa.v2.Terminal

/**
 * The conversation of a Task's agent from its own records (plan F4), followed
 * while Focus shows the Task: Chat shows it, and the Needs-you bar shows the
 * choice it waits on in every view.
 */
class ChatLink(private val model: NextModel) {
    var number by mutableStateOf<Int?>(null)
    var attempt by mutableIntStateOf(0)
        private set
    val ended: String? get() = number?.let { model.chatEnded[it] }
    val entries: List<ChatEntry>? get() = number?.let { model.chats[it] }
    val prompt: ChatPrompt? get() = number?.let { model.chatPrompts[it] }

    fun reopen() {
        number?.let(model::closeChat)
        number = null
        attempt += 1
    }
}

/** Follows [terminal]'s conversation while shown; null for a terminal without one. */
@Composable
fun rememberChat(model: NextModel, terminal: Terminal): ChatLink? {
    val link = remember(terminal.id) { ChatLink(model) }
    val follows = terminal.isTaskAgent
    LaunchedEffect(terminal.id, link.attempt, follows) { if (follows) link.number = model.openChat(terminal.id) }
    DisposableEffect(terminal.id) { onDispose { link.number?.let(model::closeChat) } }
    return link.takeIf { follows }
}

/** Chat: the conversation. Answers go to the terminal as input; muxa never answers a menu by itself. */
@Composable
fun ChatView(link: ChatLink, modifier: Modifier) {
    val ended = link.ended
    val entries = link.entries
    Column(modifier.testTag("focus.chatpanel")) {
        when {
            ended != null -> Column(
                Modifier.fillMaxSize().padding(16.dp),
                verticalArrangement = Arrangement.Center,
                horizontalAlignment = Alignment.CenterHorizontally,
            ) {
                Text(ended, color = Palette.textSecondary)
                LinkButton("Open again", link::reopen)
            }
            entries != null -> Conversation(entries, Modifier.fillMaxSize())
            else -> Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) { CircularProgressIndicator() }
        }
    }
}

@Composable
private fun Conversation(entries: List<ChatEntry>, modifier: Modifier) {
    val list = rememberLazyListState()
    LaunchedEffect(entries.size) { if (entries.isNotEmpty()) list.scrollToItem(entries.size - 1) }
    LazyColumn(modifier.testTag("chat.list"), state = list, contentPadding = PaddingValues(vertical = 8.dp)) {
        items(entries, key = { it.id }) { ChatRow(it) }
    }
}

@Composable
private fun ChatRow(e: ChatEntry) {
    val pad = Modifier.fillMaxWidth().padding(horizontal = 14.dp, vertical = 6.dp)
    when (e.kind) {
        "user" -> Box(pad, contentAlignment = Alignment.CenterEnd) {
            Text(
                e.text.orEmpty(),
                Modifier
                    .widthIn(max = 520.dp)
                    .background(Theme.userBubble, RoundedCornerShape(14.dp))
                    .padding(horizontal = 12.dp, vertical = 8.dp)
                    .testTag("chat.user"),
            )
        }
        "tool" -> Column(
            pad.background(Theme.toolCard, RoundedCornerShape(10.dp)).border(1.dp, Palette.border, RoundedCornerShape(10.dp))
                .padding(10.dp).testTag("chat.tool"),
        ) {
            Text("${e.tool?.name ?: "Tool"} ${e.tool?.summary.orEmpty()}", fontSize = 12.5.sp, color = Palette.text,
                fontFamily = FontFamily.Monospace, maxLines = 2, overflow = TextOverflow.Ellipsis)
            e.tool?.result?.let {
                Text(it, fontSize = 11.5.sp, fontFamily = FontFamily.Monospace, maxLines = 4,
                    color = if (e.tool?.error == true) Theme.failure else Palette.textSecondary)
            }
        }
        "notice" -> Text(e.text.orEmpty(), pad, fontSize = 12.sp, color = Palette.textTertiary)
        else -> Text(e.text.orEmpty(), pad.testTag("chat.assistant"), lineHeight = 22.sp)
    }
}

/**
 * The choice the agent waits on, as the amber Needs-you bar. Each option sends
 * the input the adapter checked (ADR 0009); the first is the filled one.
 */

@Composable
fun NeedsYouBar(p: ChatPrompt, pick: (String) -> Unit, modifier: Modifier = Modifier) {
    val ask = p.question ?: when (p.kind) {
        "permission" -> "the agent asks to go on"
        "question" -> "the agent asks"
        "plan" -> "the agent asks to leave plan mode"
        else -> "the agent waits"
    }
    Row(
        modifier
            .fillMaxWidth()
            .background(StateColors.needsYouBackground, RoundedCornerShape(12.dp))
            .border(1.dp, StateColors.needsYouBorder, RoundedCornerShape(12.dp))
            .padding(horizontal = 12.dp, vertical = 6.dp)
            .testTag("chat.prompt"),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Dot(StateColors.needsYou)
        Text("Needs you: $ask", Modifier.weight(1f), fontSize = 14.sp, lineHeight = 18.sp, maxLines = 3, overflow = TextOverflow.Ellipsis)
        FlowRow(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            p.options.forEachIndexed { i, o ->
                val tag = Modifier.testTag("chat.option.${o.label}")
                if (i == 0) {
                    PrimaryButton(o.label, { pick(o.input) }, tag, tone = Tone.NEEDS_YOU)
                } else {
                    SecondaryButton(o.label, { pick(o.input) }, tag)
                }
            }
        }
    }
}

/**
 * The line under a Task: typed into its terminal with Enter, as the design's
 * rounded field and round send button. [round] is the phone's pill shape.
 */
@Composable
fun Composer(model: NextModel, terminal: Terminal, round: Boolean, modifier: Modifier = Modifier) {
    var draft by rememberSaveable(terminal.id) { mutableStateOf("") }
    fun send() {
        val text = draft.trim()
        if (text.isEmpty()) return
        draft = ""
        model.send("$text\r", terminal.id)
    }
    val shape = RoundedCornerShape(if (round) 24.dp else 10.dp)
    Column(modifier) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            BasicTextField(
                draft, { draft = it },
                Modifier.weight(1f).testTag("chat.field").semantics { contentDescription = "Reply" },
                textStyle = TextStyle(color = Palette.text, fontSize = 15.sp),
                cursorBrush = SolidColor(Palette.accent),
                singleLine = true,
                decorationBox = { field ->
                    Box(
                        Modifier.heightIn(min = 48.dp).background(Palette.ground, shape).border(1.dp, Palette.borderStrong, shape)
                            .padding(horizontal = 16.dp),
                        contentAlignment = Alignment.CenterStart,
                    ) {
                        if (draft.isEmpty()) {
                            Text(
                                if (terminal.isTaskAgent) "Reply, or tell it what to change" else "Type into the terminal",
                                color = Palette.textTertiary, fontSize = 15.sp, maxLines = 1,
                            )
                        }
                        field()
                    }
                },
            )
            SendButton("Send", draft.isNotBlank(), ::send, Modifier.testTag("chat.send"))
        }
        model.requestError?.let { Text(it, Modifier.padding(top = 4.dp), color = Theme.failure, fontSize = 12.5.sp) }
    }
}
