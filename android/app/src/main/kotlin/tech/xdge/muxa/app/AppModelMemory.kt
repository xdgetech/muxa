package tech.xdge.muxa.app

import kotlinx.coroutines.launch
import tech.xdge.muxa.protocol.InboxItem
import tech.xdge.muxa.protocol.ItemKind
import tech.xdge.muxa.protocol.MemoryProposal
import tech.xdge.muxa.protocol.WorkspaceInfo
import tech.xdge.muxa.session.PaneSession
import tech.xdge.muxa.session.memoryItemOf
import tech.xdge.muxa.session.memoryOf

// Workspace memory (ADR 0053). Approve sends the hash of the text this client showed whole; the
// host appends it to the workspace's WORKSPACE.md. A lower host keeps today's UI.

/** The host keeps workspace memory and approves additions by hash (`work:14`). */
val AppModel.canKeepMemory: Boolean get() = isConnected && state.canKeepMemory

/** [workspace] with its `memory` mode and file time, at `work:14`; null otherwise. */
fun AppModel.memoryOf(workspace: String?): WorkspaceInfo? =
    if (canKeepMemory) state.tasks?.workspaces.orEmpty().memoryOf(workspace) else null

/** Task [task]'s `memory` item, at `work:14`. */
fun AppModel.memoryItemOf(task: String): InboxItem? = if (canKeepMemory) state.inboxItems.memoryItemOf(task) else null

/**
 * Approve on a `memory` item asks first, with the workspace once more. It asks only on the item
 * screen that shows the text whole; from anywhere else it opens that screen instead.
 */
fun AppModel.askApproveMemory(item: InboxItem) {
    val memory = item.memory?.takeIf { canKeepMemory && item.kind == ItemKind.MEMORY } ?: return
    if (selectedItem != item.id || paneShown) return openItem(item.id)
    inboxPrompt = InboxPrompt.ApproveMemory(item, memory)
}

/** Sends Approve with [shown]'s hash: the proposal whose text the item screen showed. */
fun AppModel.approveMemory(item: InboxItem, shown: MemoryProposal) {
    inboxPrompt = null
    if (answeringItem != null) return
    val before = inboxItems.map { it.id }
    answeringItem = item.id
    scope.launch {
        try {
            session.approveMemory(item, shown)
            afterAnswer(item.id, before)
        } catch (e: PaneSession.Failure) {
            inboxError = PaneSession.describe(e)
        } finally {
            answeringItem = null
        }
    }
}

/** Open Item from the Task detail: the proposal's item screen in the Inbox, where Approve is. */
fun AppModel.openMemoryItem(id: String) {
    taskInfo = null
    selectMode(SidebarMode.INBOX)
    openItem(id)
}
