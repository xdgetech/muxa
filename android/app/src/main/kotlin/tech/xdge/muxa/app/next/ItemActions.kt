package tech.xdge.muxa.app.next

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.launch
import tech.xdge.muxa.app.ui.DestructiveButton
import tech.xdge.muxa.app.ui.Icons
import tech.xdge.muxa.app.ui.LinkButton
import tech.xdge.muxa.app.ui.Palette
import tech.xdge.muxa.app.ui.PrimaryButton
import tech.xdge.muxa.app.ui.SecondaryButton
import tech.xdge.muxa.v2.InboxItem
import tech.xdge.muxa.v2.ItemKind
import tech.xdge.muxa.v2.Terminal
import tech.xdge.muxa.v2.V2Request

/**
 * The Task actions of an Inbox item (ADR 0072), shared by the detail's buttons
 * and Home's keys: Approve, Request changes, Retry and Resume Task.
 */
@Stable
class ItemActionState(private val model: NextModel, private val scope: CoroutineScope) {
    /** The Review item whose comment field is open. */
    var commenting: String? by mutableStateOf(null)

    /** An Approve refused because it would lose work: the item and the server's message. */
    var loss: Pair<String, String>? by mutableStateOf(null)
    var busy by mutableStateOf(false)
        private set

    /** `Ctrl+Enter`: the kind's main action. */
    fun main(item: InboxItem, done: () -> Unit) {
        when (item.kind) {
            ItemKind.REVIEW -> approve(item.id, force = false, done)
            ItemKind.FAILED -> retry(item.id, done)
            else -> {
                model.inbox(V2Request.InboxDone(item.id))
                done()
            }
        }
    }

    fun approve(item: String, force: Boolean, done: () -> Unit) = act({
        val refused = model.approve(item, force)
        loss = refused?.let { item to it }
        refused == null && model.requestError == null
    }, done)

    fun requestChanges(item: String, comment: String, done: () -> Unit) = act({
        model.inboxNow(V2Request.InboxRequestChanges(item, comment))
        (model.requestError == null).also { if (it) commenting = null }
    }, done)

    fun retry(item: String, done: () -> Unit) = act({ model.retry(item) }, done)

    /** Applies a Bot's settings proposal (ADR 0083); the item ends Done. */
    fun apply(item: InboxItem, done: () -> Unit) = act({ model.applyProposal(item) }, done)

    /** Asks [bot] about [item] in a talk, which opens in Focus with [open]. The item stays. */
    fun ask(item: InboxItem, bot: String, open: (Terminal) -> Unit) {
        val draft = TaskDraft.ask(item, bot) ?: return
        var started: Terminal? = null
        act({
            started = model.startTask(draft)
            started != null
        }) { started?.let(open) }
    }

    /** Resume Task of an item from v1; its agent opens in Focus with [open]. */
    fun resume(item: InboxItem, open: (Terminal) -> Unit, done: () -> Unit) {
        var started: Terminal? = null
        act({
            started = model.resumeTask(item)
            started != null
        }) {
            done()
            started?.let(open)
        }
    }

    /** Runs [body]; [done] follows only when it worked, so a failure stays on the item. */
    private fun act(body: suspend () -> Boolean, done: () -> Unit) {
        if (busy) return
        scope.launch {
            busy = true
            val ok = try {
                body()
            } finally {
                busy = false
            }
            if (ok) done()
        }
    }
}

/** The buttons of an open item, by kind (`docs/next.md`, Inbox actions). */
@Composable
fun ItemActions(
    model: NextModel,
    item: InboxItem,
    actions: ItemActionState,
    openTerminal: (Terminal) -> Unit,
    newTask: (TaskSeed) -> Unit,
    onDone: () -> Unit,
) {
    fun act(r: V2Request) {
        model.inbox(r)
        onDone()
    }
    val terminal = item.terminal?.let { id -> model.terminals.firstOrNull { it.id == id } }
    var later by remember { mutableStateOf(false) }
    var more by remember { mutableStateOf(false) }
    val workspace = item.workspace
    val bots by produceState(emptyList<String>(), workspace) { value = workspace?.let { model.botNames(it) }.orEmpty() }
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
            PrimaryButton(item.kind.mainAction, { actions.main(item, onDone) }, Modifier.testTag("detail.main"), enabled = !actions.busy)
            if (item.kind == ItemKind.REVIEW) {
                SecondaryButton("Request Changes", { actions.commenting = item.id }, Modifier.testTag("detail.changes"), enabled = !actions.busy)
            }
            if (item.place != null) {
                SecondaryButton("Resume Task", { actions.resume(item, openTerminal, onDone) }, Modifier.testTag("detail.resume"), enabled = !actions.busy)
            } else if (item.kind in TASK_KINDS) {
                SecondaryButton("Start Task", { newTask(TaskSeed.startTask(item)) }, Modifier.testTag("detail.starttask"))
            }
            if (item.kind == ItemKind.READ) {
                SecondaryButton("Make it a Task", { newTask(TaskSeed(item.title, item.workspace, item.note.orEmpty())) }, Modifier.testTag("detail.maketask"))
            }
            if (terminal != null) {
                SecondaryButton("Open in Focus", { openTerminal(terminal) }, Modifier.testTag("detail.open"))
            }
            if (bots.isNotEmpty()) AskBotButton(bots, !actions.busy) { actions.ask(item, it, openTerminal) }
            Column {
                SecondaryButton("Later", { later = true }, Modifier.testTag("detail.later"))
                DropdownMenu(later, { later = false }) {
                    for (choice in LaterChoice.entries) {
                        DropdownMenuItem({ Text(choice.title) }, {
                            later = false
                            act(V2Request.InboxLater(item.id, choice.until()))
                        })
                    }
                }
            }
            if (item.kind.mainAction != "Done") {
                SecondaryButton("Done", { act(V2Request.InboxDone(item.id)) }, Modifier.testTag("detail.done"))
            }
            Column {
                IconButton({ more = true }) { Icon(Icons.more, "More") }
                DropdownMenu(more, { more = false }) {
                    DropdownMenuItem({ Text("Delete", color = Palette.destructive) }, {
                        more = false
                        act(V2Request.InboxDelete(item.id))
                    })
                }
            }
        }
        if (actions.commenting == item.id) CommentField(item, actions, onDone)
    }
    actions.loss?.takeIf { it.first == item.id }?.let { (_, message) -> LoseWorkDialog(item, message, actions, onDone) }
}

/**
 * One line typed to the Task's agent. Several lines would go as a bracketed
 * paste, which no finding confirms yet (ADR 0072 §3).
 */
@Composable
private fun CommentField(item: InboxItem, actions: ItemActionState, onDone: () -> Unit) {
    var comment by remember(item.id) { mutableStateOf("") }
    val focus = remember { FocusRequester() }
    LaunchedEffect(item.id) { focus.requestFocus() }
    fun send() {
        if (comment.isNotBlank()) actions.requestChanges(item.id, comment.trim(), onDone)
    }
    Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
        OutlinedTextField(
            comment, { comment = it.replace(LINE_BREAK, " ") },
            Modifier.weight(1f).focusRequester(focus).testTag("detail.comment"),
            placeholder = { Text("What should the agent change?") },
            singleLine = true,
            keyboardOptions = KeyboardOptions(imeAction = ImeAction.Send),
            keyboardActions = KeyboardActions(onSend = { send() }),
        )
        LinkButton("Cancel", { actions.commenting = null })
        PrimaryButton("Send", ::send, Modifier.testTag("detail.comment.send"), enabled = comment.isNotBlank() && !actions.busy)
    }
}

@Composable
private fun LoseWorkDialog(item: InboxItem, message: String, actions: ItemActionState, onDone: () -> Unit) {
    AlertDialog(
        onDismissRequest = { actions.loss = null },
        title = { Text("Remove the worktrees anyway?") },
        text = { Text(message, Modifier.fillMaxWidth().testTag("detail.loss")) },
        confirmButton = {
            DestructiveButton("Remove Anyway", { actions.approve(item.id, force = true, onDone) }, Modifier.testTag("detail.force"))
        },
        dismissButton = { LinkButton("Cancel", { actions.loss = null }) },
    )
}

/** The kinds whose detail offers Start Task. */
private val TASK_KINDS = setOf(ItemKind.REPLY, ItemKind.DECIDE, ItemKind.TODO)

private val LINE_BREAK = Regex("[\r\n]+")
