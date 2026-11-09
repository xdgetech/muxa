package tech.xdge.muxa.app.ui

import androidx.compose.runtime.getValue
import androidx.compose.runtime.setValue
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.clickable
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
import androidx.compose.foundation.layout.size
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
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
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
import tech.xdge.muxa.app.canDeleteInbox
import tech.xdge.muxa.app.protect
import tech.xdge.muxa.app.tap
import tech.xdge.muxa.protocol.InboxAction
import tech.xdge.muxa.protocol.InboxItem
import tech.xdge.muxa.protocol.ItemKind
import tech.xdge.muxa.session.buttons
import tech.xdge.muxa.session.choices
import tech.xdge.muxa.session.isProtected
import tech.xdge.muxa.session.isWaiting
import tech.xdge.muxa.session.mainAnswer

/**
 * One Inbox item (docs/inbox.md, "The item screen"): the question and its source, the summary
 * and the evidence that opens in the Reader. Later, ✓ and the … menu are in the top bar, the
 * same on every item (ADR 0040); the answers to the agent and the reply box stay above the keyboard.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ItemDetail(model: AppModel, id: String?, modifier: Modifier) {
    val item = id?.let(model::item)
    val waiting = item?.takeIf { model.usesItems && it.isWaiting(System.currentTimeMillis() / 1000) }
    Column(modifier.navigationBarsPadding().imePadding()) {
        TopAppBar(
            title = { Text(item?.kindTitle ?: "Inbox", maxLines = 1, overflow = TextOverflow.Ellipsis) },
            navigationIcon = {
                if (LocalPhoneLayout.current && id != null) {
                    IconButton(model::back, Modifier.testTag("item-back")) { Icon(Icons.back, "Inbox") }
                }
            },
            actions = { if (waiting != null) Triage(model, waiting) },
        )
        when {
            !model.usesItems -> InboxEmpty(model, OLDER_HOST, "item")
            item == null && model.inboxItems.isEmpty() -> InboxEmpty(model, NOTHING, "item")
            item == null -> Placeholder("Pick a question.")
            waiting == null -> Placeholder("This question is answered.")
            else -> {
                Column(
                    Modifier.fillMaxWidth().weight(1f).verticalScroll(rememberScrollState()).padding(16.dp).testTag("item-detail"),
                    verticalArrangement = Arrangement.spacedBy(12.dp),
                ) { ItemBody(model, waiting) }
                AnswerBar(model, waiting)
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
    SourceLine(model, item)
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
            "The agent's menu can't be answered here. Open the session to answer it.",
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
}

/** The source; with a session it opens it (ADR 0040 §5). */
@Composable
private fun SourceLine(model: AppModel, item: InboxItem) {
    val text = listOf(item.routine ?: item.title, item.workspace, AgentSummary.age(item.createdAt)).joinToString(" · ")
    if (model.itemSession(item) == null) {
        Text(
            text, Modifier.testTag("item-source"),
            style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        return
    }
    Row(
        Modifier.clickable(onClickLabel = "Open Session") { model.openSession(item) }.testTag("item-open-session"),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(text, Modifier.testTag("item-source"), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.primary)
        Icon(Icons.chevron, null, Modifier.size(16.dp), tint = MaterialTheme.colorScheme.primary)
    }
}

/** Its Chat where the agent has one, else its pane; the pane switches between Chat, Agent and Shell. */
private fun AppModel.openSession(item: InboxItem) =
    openItemSession(item, if (itemHasChat(item)) AppModel.PaneView.CHAT else AppModel.PaneView.AGENT)

/** Later, ✓ and the … menu: what the developer does with the item, apart from what the agent asked. */
@Composable
private fun Triage(model: AppModel, item: InboxItem) {
    val buttons = item.buttons
    val busy = model.answeringItem != null
    if (buttons.later) {
        IconButton({ model.tap(item, InboxAction.LATER) }, Modifier.testTag("item-later"), enabled = !busy) {
            Icon(Icons.later, InboxAction.LATER.title(item), tint = MaterialTheme.colorScheme.onSurfaceVariant)
        }
    }
    buttons.done?.let { done ->
        IconButton({ model.tap(item, done) }, Modifier.testTag("item-${done.tag}"), enabled = !busy) {
            Icon(Icons.check, done.title(item), tint = Theme.done)
        }
    }
    val session = model.itemSession(item) != null
    val protects = model.canDeleteInbox
    if (buttons.menu.isEmpty() && !session && !protects) return
    var open by remember { mutableStateOf(false) }
    Box {
        IconButton({ open = true }, Modifier.testTag("item-more"), enabled = !busy) { Icon(Icons.more, "More") }
        DropdownMenu(open, { open = false }) {
            for (action in buttons.menu) {
                DropdownMenuItem(
                    { Text(action.title(item)) },
                    {
                        open = false
                        model.tap(item, action)
                    },
                    Modifier.testTag("item-more-${action.tag}"),
                )
            }
            if (protects) {
                val action = if (item.isProtected) InboxAction.UNPROTECT else InboxAction.PROTECT
                DropdownMenuItem(
                    { Text(action.title(item)) },
                    {
                        open = false
                        model.protect(item, !item.isProtected)
                    },
                    Modifier.testTag("item-more-${action.tag}"),
                    leadingIcon = { Icon(if (item.isProtected) Icons.unprotect else Icons.protect, null) },
                )
            }
            if (session) {
                DropdownMenuItem(
                    { Text("Open Session") },
                    {
                        open = false
                        model.openSession(item)
                    },
                    Modifier.testTag("item-more-open-session"),
                    leadingIcon = { Icon(Icons.newWindow, null) },
                )
            }
        }
    }
}

/** The answers to what the agent asked: a question's options, the decisions, then the reply box. */
@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun AnswerBar(model: AppModel, item: InboxItem) {
    val buttons = item.buttons
    val choices = item.choices
    val textAction = buttons.text
    if (choices.isEmpty() && buttons.decisions.isEmpty() && textAction == null) return
    val busy = model.answeringItem != null
    val text = model.reply(item)
    val main = item.mainAnswer(text)
    HorizontalDivider()
    Column(Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        if (choices.isNotEmpty()) {
            FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                choices.forEachIndexed { i, option ->
                    OutlinedButton({ model.answer(item, InboxAction.CHOOSE, option = i) }, Modifier.testTag("item-option-$i"), enabled = !busy) {
                        Text(option.label)
                    }
                }
            }
        }
        if (buttons.decisions.isNotEmpty()) {
            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                for (action in buttons.decisions) {
                    val onClick = { model.tap(item, action) }
                    val tag = Modifier.weight(1f).testTag("item-${action.tag}")
                    if (action == main) {
                        Button(onClick, tag, enabled = !busy) { Text(action.title(item)) }
                    } else {
                        OutlinedButton(onClick, tag, enabled = !busy) { Text(action.title(item)) }
                    }
                }
            }
        }
        if (textAction != null) Composer(model, item, textAction, text, busy)
    }
}

/**
 * The reply box: one field and a send arrow. [action] is Send to the item's agent, or Ask
 * Agent where the item has no session (ADR 0041); the placeholder says where the text goes.
 */
@Composable
private fun Composer(model: AppModel, item: InboxItem, action: InboxAction, text: String, busy: Boolean) {
    val placeholder = when {
        action == InboxAction.ASK_AGENT -> "Ask the workspace's agent…"
        item.kind == ItemKind.ASK -> "Deny, then tell the agent…"
        else -> "Reply to the agent…"
    }
    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.Bottom, horizontalArrangement = Arrangement.spacedBy(4.dp)) {
        OutlinedTextField(
            text, { model.setReply(item, it) }, Modifier.weight(1f).testTag("item-reply"),
            placeholder = { Text(placeholder) }, maxLines = 4,
        )
        FilledIconButton({ model.tap(item, action) }, Modifier.testTag("item-send"), enabled = !busy && text.isNotBlank()) {
            Icon(Icons.send, action.title(item))
        }
    }
}

/** The action's name on the wire, for test tags. */
val InboxAction.wire: String get() = InboxAction.serializer().descriptor.getElementName(ordinal)

/** A button's test tag: the action's wire name. */
private val InboxAction.tag: String get() = wire

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
    InboxAction.KEEP_OPEN, InboxAction.DISMISS -> "Done"
    InboxAction.RETRY -> "Retry"
    InboxAction.DROP -> "Drop Task"
    InboxAction.LATER -> "Later"
    InboxAction.CHOOSE -> "Choose"
    InboxAction.READ -> "Mark Read"
    InboxAction.ASK_AGENT -> "Ask Agent"
    InboxAction.PROTECT -> "Protect"
    InboxAction.UNPROTECT -> "Unprotect"
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
