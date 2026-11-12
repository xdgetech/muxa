package tech.xdge.muxa.demo

import tech.xdge.muxa.protocol.AssistResult
import tech.xdge.muxa.protocol.AssistantRequest
import tech.xdge.muxa.protocol.Attachments
import tech.xdge.muxa.protocol.Handoff
import tech.xdge.muxa.protocol.InboxAction
import tech.xdge.muxa.protocol.InboxItem
import tech.xdge.muxa.protocol.ItemKind
import tech.xdge.muxa.protocol.ItemState
import tech.xdge.muxa.protocol.ResultReview
import tech.xdge.muxa.protocol.SourceLink

/**
 * The demo bridge's `result` items (ADR 0058, `work` level 16): one per stored Result, answered like
 * a Task's review. The words follow the host's: "<worker> finished: …", or "<worker> asks <next> to: …".
 */
internal object DemoReviews {
    /** Why the host resolves an older version's `result` item. */
    const val REPLACED = "A newer version of this request replaced this result."

    private const val MAX_LINKS = 10
    private const val MAX_FINDING = 200

    /** [result] of [q] as a `result` item with [id]; [handoff] is the hand-off the step proposes and that can still start. */
    fun item(id: String, q: AssistantRequest, result: AssistResult, title: String, handoff: Handoff? = null): InboxItem {
        val worker = result.worker
        val who = worker ?: "The assistant"
        val question = handoff?.let { "$who asks ${it.worker} to: ${it.purpose}" }
            ?: "$who finished: ${result.summary.lineSequence().first()}"
        val links = result.findings.flatMap { it.sources }
            .mapNotNull { s -> s.url?.let { SourceLink(it, s.title) } }
            .distinctBy { it.url }
            .take(MAX_LINKS)
        return InboxItem(
            id, ItemKind.RESULT, ItemState.OPEN, question, q.workspace, title, result.at,
            summary = result.summary,
            attachments = Attachments(links = links),
            result = ResultReview(
                q.id, result.version, result.step ?: 1, q.work, worker,
                findings = result.findings.map { it.text.lineSequence().first().take(MAX_FINDING) },
                openQuestions = result.openQuestions, next = result.next, files = result.files, handoff = handoff,
            ),
            answers = listOfNotNull(InboxAction.APPROVE.takeIf { handoff != null }) + ANSWERS,
        )
    }

    private val ANSWERS = listOf(InboxAction.REPLY, InboxAction.DISMISS, InboxAction.LATER)

    /** A `write` item's answers at level 16: Revise is `reply` (ADR 0058 §4). */
    val WRITE_ANSWERS = listOf(InboxAction.APPROVE, InboxAction.REJECT, InboxAction.REPLY, InboxAction.LATER)
}
