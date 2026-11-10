package tech.xdge.muxa.app

import kotlinx.coroutines.launch
import tech.xdge.muxa.protocol.DelegateInfo
import tech.xdge.muxa.protocol.InboxItem
import tech.xdge.muxa.protocol.ItemKind
import tech.xdge.muxa.protocol.TaskRecord
import tech.xdge.muxa.protocol.WriteProposal
import tech.xdge.muxa.session.PaneSession
import tech.xdge.muxa.session.writesOf

// Write proposals (ADR 0052). Approve sends the hash of the write this client showed whole; the host
// starts the writer's Task. A lower host keeps today's UI: no write view, no writes in Work detail.

/** The host takes write proposals and approves them by hash (`work:13`). */
val AppModel.canWrite: Boolean get() = isConnected && state.canWrite

/** [workspace]'s writer and `writes` mode, at `work:13`; null otherwise. */
fun AppModel.writesOf(workspace: String?): DelegateInfo? = if (canWrite) state.assistant.writesOf(workspace) else null

/**
 * Approve on a `write` item asks first, with the target once more. It asks only on the item screen
 * that shows the content whole; from anywhere else it opens that screen instead.
 */
fun AppModel.askApproveWrite(item: InboxItem) {
    val write = item.write?.takeIf { canWrite && item.kind == ItemKind.WRITE } ?: return
    if (selectedItem != item.id || paneShown) return openItem(item.id)
    inboxPrompt = InboxPrompt.ApproveWrite(item, write)
}

/** Sends Approve with [shown]'s hash: the write whose content the item screen showed. */
fun AppModel.approveWrite(item: InboxItem, shown: WriteProposal) {
    inboxPrompt = null
    if (answeringItem != null) return
    val before = inboxItems.map { it.id }
    answeringItem = item.id
    scope.launch {
        try {
            session.approveWrite(item, shown)
            afterAnswer(item.id, before)
        } catch (e: PaneSession.Failure) {
            inboxError = PaneSession.describe(e)
        } finally {
            answeringItem = null
        }
    }
}

/** The writer's Task, as last listed. */
fun AppModel.taskRecord(id: String): TaskRecord? = state.tasks?.records?.firstOrNull { it.id == id }

/** Open Item from Work detail: the write's item screen, where Approve is. */
fun AppModel.openWriteItem(id: String) {
    workDetail = null
    openItem(id)
}
