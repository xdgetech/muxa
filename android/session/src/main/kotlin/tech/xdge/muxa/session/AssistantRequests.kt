package tech.xdge.muxa.session

import tech.xdge.muxa.protocol.AssistResult
import tech.xdge.muxa.protocol.AssistScope
import tech.xdge.muxa.protocol.AssistantRequest
import tech.xdge.muxa.protocol.Capture
import tech.xdge.muxa.protocol.Delivery
import tech.xdge.muxa.protocol.WorkItem

// Assistant requests as the clients show them (ADR 0046 §6, §10). The host derives `delivery`;
// this only picks the words and which actions apply.

/** One Work item as `get-work` read it: every Capture it links, with its text, and its requests. */
data class WorkDetail(val item: WorkItem, val captures: List<Capture>, val requests: List<AssistantRequest>)

/** What `capture {assist}` did. The Capture is saved even when [refusal] says the assistant did not start. */
data class Assisted(
    val capture: Capture,
    val work: WorkItem? = null,
    val request: AssistantRequest? = null,
    val refusal: PaneSession.Failure? = null,
)

/** The scopes the clients offer, in this order. */
val ASSIST_SCOPES = listOf(AssistScope.RESEARCH, AssistScope.COMPARE, AssistScope.DRAFT)

/** At most this many bytes in a request's purpose (PROTOCOL.md). */
const val PURPOSE_MAX_BYTES = 4096

val AssistScope.label: String
    get() = when (this) {
        AssistScope.RESEARCH -> "Research"
        AssistScope.COMPARE -> "Compare"
        AssistScope.DRAFT -> "Draft"
        else -> raw
    }

/** A short phrase for a row or the detail. An unknown value reads as unknown, as PROTOCOL.md says. */
val Delivery.label: String
    get() = when (this) {
        Delivery.PENDING -> "waiting to send"
        Delivery.RECEIVED -> "received"
        Delivery.PROCESSING -> "working"
        Delivery.UNCONFIRMED -> "send not confirmed"
        Delivery.RESULTED -> "result ready"
        Delivery.FAILED -> "failed"
        Delivery.TIMED_OUT -> "timed out"
        Delivery.CANCEL_REQUESTED -> "cancelling"
        Delivery.CANCELLED -> "cancelled"
        else -> "unknown"
    }

/** The run of the current version may still be alive: no new version is asked while it is. */
val AssistantRequest.isActive: Boolean get() = delivery in ACTIVE

/** Cancel stops the current version's run while it has no result. */
val AssistantRequest.canCancel: Boolean get() = delivery in CANCELLABLE && results.none { it.version == version }

/** The run's session lives while its prompt is queued or the agent works; Open Session shows it. */
val AssistantRequest.runMayLive: Boolean get() = run != null && delivery in CANCELLABLE

/** Results that are not stale, newest first. */
val AssistantRequest.currentResults: List<AssistResult> get() = results.filterNot { it.stale }

/** Older versions' results, kept as history under "Earlier". */
val AssistantRequest.earlierResults: List<AssistResult> get() = results.filter { it.stale }

/** [item]'s requests in its order; a listed one wins over [fallback], which `get-work` read. */
fun requestsOf(item: WorkItem, listed: List<AssistantRequest>, fallback: List<AssistantRequest> = emptyList()): List<AssistantRequest> =
    item.requests.mapNotNull { id -> listed.firstOrNull { it.id == id } ?: fallback.firstOrNull { it.id == id } }

/** The Work row's note on its newest request: "Result ready", else its delivery state. Null without one. */
fun assistNote(item: WorkItem, listed: List<AssistantRequest>): String? {
    val newest = requestsOf(item, listed).maxByOrNull { it.updatedAt } ?: return null
    return if (newest.delivery == Delivery.RESULTED) RESULT_READY else "assistant ${newest.delivery.label}"
}

/** The Work row's marker for a request with a result. */
const val RESULT_READY = "Result ready"

/** What to fix before asking; null when it can be sent. */
fun assistProblem(workspace: String?, purpose: String?): String? = when {
    workspace.isNullOrEmpty() -> "Pick a workspace for the assistant."
    (purpose?.toByteArray()?.size ?: 0) > PURPOSE_MAX_BYTES -> "The purpose is too long. Keep it under 4 KiB."
    else -> null
}

private val CANCELLABLE = setOf(Delivery.PENDING, Delivery.RECEIVED, Delivery.PROCESSING, Delivery.UNCONFIRMED)
private val ACTIVE = CANCELLABLE + Delivery.CANCEL_REQUESTED
