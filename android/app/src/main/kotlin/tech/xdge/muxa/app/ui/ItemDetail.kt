package tech.xdge.muxa.app.ui

import androidx.compose.runtime.getValue
import androidx.compose.runtime.setValue
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilledIconButton
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedIconButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import tech.xdge.muxa.app.AppModel
import tech.xdge.muxa.app.item
import tech.xdge.muxa.app.itemSession
import tech.xdge.muxa.app.back
import tech.xdge.muxa.app.itemHasChat
import tech.xdge.muxa.app.openItemSession
import tech.xdge.muxa.app.reply
import tech.xdge.muxa.app.setReply
import tech.xdge.muxa.app.answer
import tech.xdge.muxa.app.tap
import tech.xdge.muxa.protocol.InboxAction
import tech.xdge.muxa.protocol.InboxItem
import tech.xdge.muxa.protocol.ItemKind
import tech.xdge.muxa.session.buttons
import tech.xdge.muxa.session.choices
import tech.xdge.muxa.session.isWaiting
import tech.xdge.muxa.session.mainAnswer
import tech.xdge.muxa.session.needsText
import tech.xdge.muxa.session.onlyAsksAgent

/**
 * One Inbox item (ADR 0033): the question and its source, the summary, the evidence that
 * opens in the Reader, a reply box and the answers. The answers stay above the keyboard.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ItemDetail(model: AppModel, id: String?, modifier: Modifier) {
    val item = id?.let(model::item)
    Column(modifier.navigationBarsPadding().imePadding()) {
        TopAppBar(
            title = { Text(item?.kindTitle ?: "Inbox", maxLines = 1, overflow = TextOverflow.Ellipsis) },
            navigationIcon = {
                if (LocalPhoneLayout.current && id != null) {
                    IconButton(model::back, Modifier.testTag("item-back")) { Icon(Icons.back, "Inbox") }
                }
            },
        )
        when {
            !model.usesItems -> InboxEmpty(model, OLDER_HOST, "item")
            item == null && model.inboxItems.isEmpty() -> InboxEmpty(model, NOTHING, "item")
            item == null -> Placeholder("Pick a question.")
            !item.isWaiting(System.currentTimeMillis() / 1000) -> Placeholder("This question is answered.")
            else -> {
                Column(
                    Modifier.fillMaxWidth().weight(1f).verticalScroll(rememberScrollState()).padding(16.dp).testTag("item-detail"),
                    verticalArrangement = Arrangement.spacedBy(12.dp),
                ) { ItemBody(model, item) }
                HorizontalDivider()
                AnswerBar(model, item)
            }
        }
    }
}

@Composable
private fun Placeholder(text: String) {
    Box(Modifier.fillMaxSize().padding(32.dp), contentAlignment = Alignment.Center) {
        Text(text, color = MaterialTheme.colorScheme.onSurfaceVariant)
    }
}

@Composable
private fun ItemBody(model: AppModel, item: InboxItem) {
    Text(item.question, Modifier.testTag("item-question"), style = MaterialTheme.typography.titleLarge)
    Text(
        listOf(item.routine ?: item.title, item.workspace, AgentSummary.age(item.createdAt)).joinToString(" · "),
        Modifier.testTag("item-source"),
        style = MaterialTheme.typography.bodySmall,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
    )
    item.prompt?.question?.let { Text(it, Modifier.testTag("item-prompt")) }
    item.summary?.let { ItemSummary(model, item, it) }
    item.proposal?.let { p ->
        Text("Proposed action", style = MaterialTheme.typography.titleSmall)
        Field("Action", p.action)
        Field("Target", p.target, mono = true)
        Field("Content", p.content, mono = true)
    }
    ItemAttachments(model, item)
    if (item.kind == ItemKind.ASK && item.prompt == null) {
        Text(
            "The agent's menu can't be answered here. Open the terminal to answer it.",
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
}

/** The reply box, a question's options, the text answers, More, ✓ and Later (ADR 0040). */
@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun AnswerBar(model: AppModel, item: InboxItem) {
    val buttons = item.buttons
    val busy = model.answeringItem != null
    val text = model.reply(item)
    Column(Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        if (item.answers.any { it.needsText }) {
            OutlinedTextField(
                text, { model.setReply(item, it) }, Modifier.fillMaxWidth().testTag("item-reply"),
                placeholder = { Text(if (item.onlyAsksAgent) "Ask the workspace's agent…" else "Reply…") }, maxLines = 4,
            )
        }
        val choices = item.choices
        if (choices.isNotEmpty()) {
            FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                choices.forEachIndexed { i, option ->
                    OutlinedButton({ model.answer(item, InboxAction.CHOOSE, option = i) }, Modifier.testTag("item-option-$i"), enabled = !busy) {
                        Text(option.label)
                    }
                }
            }
        }
        val main = item.mainAnswer(text)
        FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.Center) {
            for (action in buttons.main) {
                val enabled = !busy && (!action.needsText || text.isNotBlank())
                val onClick = { model.tap(item, action) }
                val tag = Modifier.testTag("item-${action.tag}")
                if (action == main) {
                    Button(onClick, tag, enabled = enabled) { Text(action.title(item)) }
                } else {
                    OutlinedButton(onClick, tag, enabled = enabled) { Text(action.title(item)) }
                }
            }
            More(model, item, buttons.more, busy, text)
            buttons.done?.let { IconAnswer(model, item, it, Icons.check, prominent = it == main, enabled = !busy) }
            if (buttons.later) IconAnswer(model, item, InboxAction.LATER, Icons.later, prominent = false, enabled = !busy)
        }
        if (model.itemSession(item) != null) {
            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
                SessionButtons(model, item)
            }
        }
    }
}

/** ✓ and Later: icon buttons whose title is their content description, never under More. */
@Composable
private fun IconAnswer(model: AppModel, item: InboxItem, action: InboxAction, icon: ImageVector, prominent: Boolean, enabled: Boolean) {
    val onClick = { model.tap(item, action) }
    val tag = Modifier.testTag("item-${action.tag}")
    if (prominent) {
        FilledIconButton(onClick, tag, enabled = enabled) { Icon(icon, action.title(item)) }
    } else {
        OutlinedIconButton(onClick, tag, enabled = enabled) { Icon(icon, action.title(item)) }
    }
}

/**
 * Chat and Terminal: the item's session over the item (ADR 0035). Chat answers an `ask`
 * there; Terminal is the agent's terminal, where a `failure` is read. Chat shows only when
 * the agent has it.
 */
@Composable
private fun SessionButtons(model: AppModel, item: InboxItem) {
    if (model.itemSession(item) == null) return
    if (model.itemHasChat(item)) {
        OutlinedButton({ model.openItemSession(item, AppModel.PaneView.CHAT) }, Modifier.testTag("item-chat")) {
            Icon(Icons.chat, null)
            Text("Chat", Modifier.padding(start = 6.dp))
        }
    }
    OutlinedButton({ model.openItemSession(item, AppModel.PaneView.AGENT) }, Modifier.testTag("item-terminal")) {
        Icon(Icons.agent, null)
        Text("Terminal", Modifier.padding(start = 6.dp))
    }
}

@Composable
private fun More(model: AppModel, item: InboxItem, actions: List<InboxAction>, busy: Boolean, text: String) {
    if (actions.isEmpty()) return
    var open by remember { mutableStateOf(false) }
    Box {
        IconButton({ open = true }, Modifier.testTag("item-more")) { Icon(Icons.more, "More") }
        DropdownMenu(open, { open = false }) {
            for (action in actions) {
                DropdownMenuItem(
                    { Text(action.title(item)) },
                    {
                        open = false
                        model.tap(item, action)
                    },
                    Modifier.testTag("item-more-${action.tag}"),
                    enabled = !busy && (!action.needsText || text.isNotBlank()),
                )
            }
        }
    }
}

/** The action's name on the wire, for test tags. */
val InboxAction.wire: String get() = InboxAction.serializer().descriptor.getElementName(ordinal)

/** A button's test tag. Send is `send`: `item-reply` is the reply box. */
private val InboxAction.tag: String get() = if (this == InboxAction.REPLY) "send" else wire

/**
 * Labels name the result (ADR 0033 §4a, ADR 0040). A proposal keeps Approve and Reject; an
 * answer that changes the Task names it; ✓ is "Done" for TalkBack.
 */
fun InboxAction.title(item: InboxItem): String = when (this) {
    InboxAction.APPROVE -> when (item.kind) {
        ItemKind.PROPOSAL -> "Approve…"
        ItemKind.ASK -> "Allow"
        else -> "Approve"
    }
    InboxAction.REJECT -> if (item.kind == ItemKind.ASK) "Deny" else "Reject"
    InboxAction.REPLY -> if (item.kind == ItemKind.ASK) "Deny & Send" else "Send"
    InboxAction.ACCEPT -> "Complete Task"
    InboxAction.REDO -> "Redo"
    InboxAction.CHANGE_COURSE -> "Change Course"
    InboxAction.KEEP_OPEN, InboxAction.DISMISS -> "Done"
    InboxAction.RETRY -> "Retry"
    InboxAction.DROP -> "Drop Task"
    InboxAction.LATER -> "Later"
    InboxAction.CHOOSE -> "Choose"
    InboxAction.READ -> "Mark Read"
    InboxAction.ASK_AGENT -> "Ask Agent"
}

val InboxItem.kindTitle: String
    get() = when (kind) {
        ItemKind.ASK -> "Question"
        ItemKind.REVIEW -> "Review"
        ItemKind.FAILURE -> "Failure"
        ItemKind.PROPOSAL -> "Proposal"
        ItemKind.BRIEFING -> "Briefing"
        else -> kind.raw
    }
