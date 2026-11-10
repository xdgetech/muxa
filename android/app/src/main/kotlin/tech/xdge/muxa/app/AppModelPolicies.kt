package tech.xdge.muxa.app

import tech.xdge.muxa.protocol.InboxAction
import tech.xdge.muxa.protocol.InboxItem
import tech.xdge.muxa.protocol.ItemKind
import tech.xdge.muxa.protocol.Policy
import tech.xdge.muxa.session.Memory
import tech.xdge.muxa.session.PaneSession
import tech.xdge.muxa.session.PolicyDraft
import tech.xdge.muxa.session.moving
import java.time.ZoneId

// Policies, corrections and hypotheses (ADR 0050): what the user told the overall assistant, and
// what the host saw. The host keeps and checks them; each call returns the host's reason when it refused.

/** The host keeps policies; Today takes reorder and Not today (`work:11`). */
val AppModel.canKeepPolicies: Boolean get() = isConnected && state.canKeepPolicies

/** What the host remembers, as last listed; empty below `work:11`. */
val AppModel.memory: Memory get() = if (canKeepPolicies) state.memory else Memory()

/** A Today row's or a correction's words: the Work item's title or the Inbox item's question. */
fun AppModel.rowTitle(id: String): String = workItem(id)?.title ?: item(id)?.question ?: id

/** The workspaces a policy may name: not the overall assistant's (ADR 0050 §7). */
val AppModel.policyScopes: List<String> get() = pickableWorkspaces.map { it.name }

suspend fun AppModel.addPolicy(draft: PolicyDraft, zone: ZoneId = ZoneId.systemDefault()): String? = refusal {
    val now = System.currentTimeMillis() / 1000
    session.addPolicy(draft.text.trim(), draft.scope, draft.from(zone), draft.until(now, zone))
}

/**
 * Sends only the fields [draft] changed on [policy]. The protocol cannot clear a start or an end
 * yet, so removing one asks the user to add the policy again.
 */
suspend fun AppModel.editPolicy(policy: Policy, draft: PolicyDraft, zone: ZoneId = ZoneId.systemDefault()): String? {
    val was = PolicyDraft.of(policy, zone)
    if ((policy.from != null && draft.from == null) || (policy.until != null && draft.until(0, zone) == null)) return CANNOT_CLEAR
    if (draft.copy(text = draft.text.trim()) == was) return null
    val now = System.currentTimeMillis() / 1000
    return refusal {
        session.editPolicy(
            policy,
            text = draft.text.trim().takeIf { it != policy.text },
            scope = draft.scope.takeIf { it != policy.scope },
            from = draft.from(zone).takeIf { draft.from != was.from },
            until = draft.until(now, zone).takeIf { draft.end != was.end },
        )
    }
}

suspend fun AppModel.deletePolicy(policy: Policy): String? = refusal { session.deletePolicy(policy) }

/** Forgets a correction or a hypothesis; the next brief cannot carry it. */
suspend fun AppModel.forget(record: String): String? = refusal { session.forget(record) }

/** Moves Today's row [target] up (-1) or down (+1) on the plan shown. */
suspend fun AppModel.moveTodayRow(target: String, offset: Int): String? {
    val plan = shownPlan ?: return null
    val order = plan.moving(target, offset) ?: return null
    return refusal { session.reorderPlan(plan, order) }
}

/** Not today: the row collapses to one line; [reason] is optional and never required. */
suspend fun AppModel.dismissTodayRow(target: String, reason: String): String? {
    val plan = shownPlan ?: return null
    return refusal { session.dismissRow(plan, target, reason.trim().ifEmpty { null }) }
}

/** Reject on the assistants' items, and on a write, takes an optional reason at `work:11`; elsewhere it goes now. */
fun AppModel.asksRejectReason(item: InboxItem): Boolean = canKeepPolicies && item.kind in REASONED_KINDS

/** Sends Reject with the user's [reason], if any. */
fun AppModel.reject(item: InboxItem, reason: String) {
    inboxPrompt = null
    sendAnswer(item, InboxAction.REJECT, reason.trim().ifEmpty { null }, null)
}

private suspend fun refusal(call: suspend () -> Unit): String? = try {
    call()
    null
} catch (e: PaneSession.Failure) {
    PaneSession.describe(e)
}

private val REASONED_KINDS = setOf(ItemKind.SORT, ItemKind.DELEGATE, ItemKind.HYPOTHESIS, ItemKind.WRITE)

private const val CANNOT_CLEAR = "A start or an end can't be removed yet. Delete the policy and add it again."
