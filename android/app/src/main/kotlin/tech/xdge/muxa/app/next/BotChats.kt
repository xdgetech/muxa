package tech.xdge.muxa.app.next

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.ui.graphics.Color
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlinx.coroutines.launch
import tech.xdge.muxa.app.ui.Icons
import tech.xdge.muxa.app.ui.LinkButton
import tech.xdge.muxa.app.ui.LocalPhoneLayout
import tech.xdge.muxa.app.ui.Palette
import tech.xdge.muxa.app.ui.PrimaryButton
import tech.xdge.muxa.app.ui.SecondaryButton
import tech.xdge.muxa.app.ui.Theme
import tech.xdge.muxa.v2.InboxItem
import tech.xdge.muxa.v2.Terminal
import java.time.LocalTime
import java.time.format.DateTimeFormatter

/**
 * A Bot's chats (ADR 0086): its live talks with New chat, then its runs. A closed chat leaves no
 * record, so only live ones show. Picking one opens it on the page.
 */
@Composable
fun BotChats(model: NextModel, e: BotEntry, bots: BotNav) {
    val scope = rememberCoroutineScope()
    var choosing by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }
    // The draft is a parameter: Compose remembers a button's lambda by what it captures.
    fun start(draft: TaskDraft) = scope.launch {
        val t = model.startTask(draft)
        if (t == null) error = model.requestError else bots.terminal = t.id
    }
    val draft = TaskDraft.talk(e.workspace.name, e.name)
    PageColumn {
        PrimaryButton("New chat", {
            if (e.talks.isEmpty()) start(draft.copy(title = newChatTitle(draft.bot.orEmpty()))) else choosing = true
        }, Modifier.testTag("bot.chats.new"))
        error?.let { Text(it, Modifier.testTag("bot.chats.error"), color = Theme.failure, fontSize = 13.sp) }
        if (e.talks.isEmpty()) Quiet("No open chats.")
        for (t in e.talks) ChatRow(model, t, e.name) { bots.terminal = t.id }
        if (e.runs.isNotEmpty()) {
            Text("Runs · ${e.runs.size}", Modifier.padding(start = 6.dp, top = 10.dp), color = Palette.textSecondary, fontSize = 14.sp)
            for (t in e.runs) ChatRow(model, t, e.name) { bots.terminal = t.id }
        }
    }
    if (choosing) {
        AlertDialog(
            onDismissRequest = { choosing = false },
            modifier = Modifier.testTag("bot.chats.choose"),
            title = { Text("New chat") },
            text = {
                Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    Text("${e.name} has an open chat. Go on with it, or start one for a new job.")
                    for (t in e.talks) {
                        SecondaryButton("Continue ${t.title}", {
                            choosing = false
                            bots.terminal = t.id
                        }, Modifier.testTag("bot.chats.continue.${t.id}"))
                    }
                }
            },
            confirmButton = {
                PrimaryButton("Start a New Chat", {
                    choosing = false
                    start(draft.copy(title = newChatTitle(draft.bot.orEmpty())))
                }, Modifier.testTag("bot.chats.start"))
            },
            dismissButton = { LinkButton("Cancel", { choosing = false }) },
        )
    }
}

/**
 * "Talk to <bot>…" on Home: Send starts a chat whose first message is the text, then opens it in Chats.
 * Its Chat waits for the agent's session as New Bot's first chat does.
 */
@Composable
fun TalkField(model: NextModel, e: BotEntry, bots: BotNav) {
    val scope = rememberCoroutineScope()
    var text by rememberSaveable(e.pick) { mutableStateOf("") }
    var busy by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }
    // Every value it uses is a parameter: Compose remembers a lambda by what it captures.
    fun send(draft: TaskDraft) = scope.launch {
        busy = true
        val t = model.startTask(draft)
        busy = false
        if (t == null) {
            error = model.requestError
        } else {
            text = ""
            error = null
            bots.terminal = t.id
            bots.selectTab(BotTab.CHATS)
        }
    }
    val draft = TaskDraft.talk(e.workspace.name, e.name).copy(title = newChatTitle(e.name), goal = text.trim())
    val canSend = text.isNotBlank() && !busy
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        OutlinedTextField(
            text, { text = it }, Modifier.weight(1f).testTag("bot.home.talk"),
            placeholder = { Text("Talk to ${e.name}…") },
            singleLine = true,
            enabled = !busy,
            keyboardOptions = KeyboardOptions(imeAction = ImeAction.Send),
            keyboardActions = KeyboardActions(onSend = { if (canSend) send(draft) }),
        )
        SendButton("Send", canSend, { send(draft) }, Modifier.testTag("bot.home.talk.send"))
    }
    error?.let { Text(it, Modifier.testTag("bot.home.talk.error"), color = Theme.failure, fontSize = 13.sp) }
}

/** A time in the title keeps a new chat apart from the open ones: a folder Task with the same title is the same Task. */
private fun newChatTitle(bot: String) = "Talk to $bot · " + LocalTime.now().format(DateTimeFormatter.ofPattern("HH:mm:ss"))

@Composable
private fun ChatRow(model: NextModel, t: Terminal, bot: String, open: () -> Unit) {
    val waiting = model.inbox.items.any { it.terminal == t.id && it.settingsProposal != null }
    PageCard("bot.chat.${t.id}", open) {
        Dot(if (waiting) Palette.accent else t.state.color)
        Column(Modifier.weight(1f)) {
            Text(t.title, fontSize = 15.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
            Text(
                if (waiting) "proposal waiting" else t.botRole(bot), fontSize = 13.sp,
                color = if (waiting) Palette.accent else Palette.textTertiary,
            )
        }
    }
}

/** What a conversation on a Bot's page shows. */
private enum class TalkView { CHAT, TERMINAL, READER }

/**
 * One talk, run or member on its Bot's page: Chat, Terminal and Reader over the attached terminal, as
 * Focus shows a Task, with the talk's proposal above. Chat serves a Task's agent only, so a run or a
 * member opens on Terminal, where the user answers its menu.
 */
@Composable
fun BotConversation(model: NextModel, bot: BotPick, terminal: Terminal, back: () -> Unit) {
    val phone = LocalPhoneLayout.current
    var view by rememberSaveable(terminal.id) { mutableStateOf(if (terminal.isTaskAgent) TalkView.CHAT else TalkView.TERMINAL) }
    val chat = rememberChat(model, terminal)
    val canRead = terminal.taskDir != null
    val shown = when {
        view == TalkView.CHAT && chat == null -> TalkView.TERMINAL
        view == TalkView.READER && !canRead -> TalkView.TERMINAL
        else -> view
    }
    val order = listOf(TalkView.CHAT, TalkView.TERMINAL, TalkView.READER)
    val segments = listOf(
        Segment("Chat", "bot.view.chat", chat != null),
        Segment("Terminal", "bot.view.terminal"),
        Segment("Reader", "bot.view.reader", canRead),
    )
    val proposal = model.inbox.items.firstOrNull { it.terminal == terminal.id && it.settingsProposal != null }
    Column(Modifier.fillMaxSize().testTag("bot.conversation")) {
        Row(Modifier.fillMaxWidth().padding(start = 4.dp, end = 12.dp, top = 4.dp), verticalAlignment = Alignment.CenterVertically) {
            IconButton(back, Modifier.testTag("bot.conversation.back")) { Icon(Icons.backChevron, bot.name, tint = Palette.accent) }
            Column(Modifier.weight(1f)) {
                Text("${bot.workspace} · ${terminal.botRole(bot.name)}", fontSize = 12.sp, color = Palette.textSecondary, maxLines = 1)
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    Text(terminal.title, Modifier.weight(1f, fill = false), fontSize = 17.sp, fontWeight = FontWeight.SemiBold,
                        maxLines = 1, overflow = TextOverflow.Ellipsis)
                    TalkChip(bot.name, Modifier.testTag("bot.conversation.bot"))
                }
            }
        }
        Segmented(segments, order.indexOf(shown), { view = order[it] }, Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 8.dp))
        proposal?.let { key(it.id) { ProposalBar(model, it, Modifier.padding(horizontal = 12.dp).padding(bottom = 8.dp)) } }
        chat?.prompt?.let { p -> NeedsYouBar(p, { model.send(it, terminal.id) }, Modifier.padding(horizontal = 12.dp).padding(bottom = 8.dp)) }
        val inset = if (phone) Modifier.padding(horizontal = 6.dp) else Modifier.padding(horizontal = 12.dp)
        Box(Modifier.weight(1f).fillMaxWidth().then(inset).clip(RoundedCornerShape(10.dp))) {
            key(terminal.id) { TerminalPane(model, terminal.id, Modifier.fillMaxSize()) }
            // Chat and the Reader cover the terminal, which stays attached at its size so answers reach it.
            val cover = Modifier.fillMaxSize().background(Palette.ground)
            when (shown) {
                TalkView.CHAT -> chat?.let { ChatView(it, cover) }
                TalkView.READER -> ReaderPanel(model, terminal, cover)
                TalkView.TERMINAL -> {}
            }
        }
        Composer(model, terminal, round = phone, modifier = Modifier.fillMaxWidth().padding(horizontal = 10.dp, vertical = 10.dp))
    }
}

/**
 * A talk's settings proposal above the conversation (ADR 0086 decision 5). It is the same item as in the
 * Inbox: Apply sends `inbox-apply`, and applying either one closes both.
 */
@Composable
private fun ProposalBar(model: NextModel, item: InboxItem, modifier: Modifier) {
    val scope = rememberCoroutineScope()
    var busy by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }
    val p = item.settingsProposal ?: return
    Column(
        modifier.fillMaxWidth().background(PROPOSAL, SHAPE).border(1.dp, Palette.accent.copy(alpha = 0.5f), SHAPE).padding(12.dp)
            .testTag("bot.proposal"),
        verticalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        Text(item.title, fontSize = 14.sp, fontWeight = FontWeight.SemiBold)
        p.ops.forEach { Text("• " + proposalLine(it), fontSize = 13.sp, color = Palette.textSecondary) }
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            PrimaryButton("Apply", {
                scope.launch {
                    busy = true
                    error = if (model.applyProposal(item)) null else model.requestError
                    busy = false
                }
            }, Modifier.testTag("bot.proposal.apply"), enabled = !busy)
            Text("Nothing changes until you apply it.", Modifier.weight(1f), fontSize = 12.sp, color = Palette.textTertiary)
        }
        error?.let { Text(it, Modifier.testTag("bot.proposal.error"), color = Theme.failure, fontSize = 13.sp) }
    }
}

private val SHAPE = RoundedCornerShape(12.dp)
private val PROPOSAL: Color get() = Palette.accent.copy(alpha = 0.12f)
