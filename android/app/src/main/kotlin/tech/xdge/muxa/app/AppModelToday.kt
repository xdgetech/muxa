package tech.xdge.muxa.app

import tech.xdge.muxa.protocol.InboxAction
import tech.xdge.muxa.protocol.InboxItem
import tech.xdge.muxa.protocol.Message
import tech.xdge.muxa.protocol.PlanRow
import tech.xdge.muxa.protocol.RunBudget
import tech.xdge.muxa.session.PaneSession
import tech.xdge.muxa.session.TodaySend
import tech.xdge.muxa.session.opensSession
import tech.xdge.muxa.session.todaySend
import tech.xdge.muxa.session.workSend

// Today rows say what to do (ADR 0059 §7): the verb, the primary action, and Send that names who
// gets the text. A failure is the host's reason, shown where the user sent from.

/** The row's Inbox item as listed; null for a Work item's row, or an item no longer listed. */
fun AppModel.rowItem(row: PlanRow): InboxItem? = row.item?.let(::item)

/** Send on a Today row: the item's reply or Ask Agent, or the secretary for a Work item at `work:17`. */
fun AppModel.rowSend(row: PlanRow): TodaySend? = row.work?.let { workSend(it, state.secretary) } ?: rowItem(row)?.todaySend

/** The overall workspace's runs left today, beside Re-rank (ADR 0059 §5); null on an older host. */
val AppModel.overallRuns: RunBudget? get() = if (canPlan) state.overallRuns else null

/**
 * The row's primary action (ADR 0059 §3): a `review` or `ask` whose Task has a session opens it, as
 * Enter on the item does; any other row opens its target.
 */
fun AppModel.doRow(row: PlanRow) {
    val item = rowItem(row)
    if (item != null && item.opensSession && itemSession(item) != null) {
        seenPlan = shownPlan?.id
        openItemSession(item, if (itemHasChat(item)) AppModel.PaneView.CHAT else AppModel.PaneView.AGENT)
        return
    }
    openPlanRow(row)
}

/**
 * Sends [text] to whom [send] names. Ask Agent and the secretary open the session's Chat, as Ask
 * Agent and Talk to Assistant do. Returns the host's reason when it refused.
 */
suspend fun AppModel.sendToday(send: TodaySend, text: String): String? = try {
    when (send) {
        is TodaySend.Reply -> {
            session.answer(send.item, InboxAction.REPLY, text)
            null
        }
        is TodaySend.AskAgent -> openChat(session.askAgent(send.item, text))
        is TodaySend.Secretary -> openChat(session.sendToSecretary(send.work, text))
    }
} catch (e: PaneSession.Failure) {
    PaneSession.describe(e)
}

private fun AppModel.openChat(started: Message.SessionStarted): String? {
    val s = state.sessions.firstOrNull { it.id == started.session } ?: return null
    workDetail = null
    openPane(s, AppModel.PaneView.CHAT)
    return null
}
