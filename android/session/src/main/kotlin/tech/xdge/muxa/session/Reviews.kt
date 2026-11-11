package tech.xdge.muxa.session

import tech.xdge.muxa.protocol.InboxItem
import tech.xdge.muxa.protocol.ItemKind

// A request's Result reviewed like a Task (ADR 0058 §7). The wire reuses `approve`, `reply` and
// `dismiss`; this only picks the words the item screen puts on them.

/** Who Send or Revise asks: the member who wrote the Result or proposed the write; null for the workspace's assistant. */
val InboxItem.reviewWorker: String?
    get() = when (kind) {
        ItemKind.RESULT -> result?.worker
        ItemKind.WRITE -> write?.worker
        else -> null
    }

/** The member a `result` item's `approve` starts: "Hand off to work-review". Null without a hand-off. */
val InboxItem.handOffTitle: String? get() = result?.handoff?.let { "Hand off to ${it.worker}" }

/** The reply box's words on a `result` or `write` item; null on other kinds, which keep the agent's. */
val InboxItem.reviewPlaceholder: String?
    get() {
        val who = reviewWorker ?: "the assistant"
        return when (kind) {
            ItemKind.RESULT -> "Tell $who what to do next…"
            ItemKind.WRITE -> "Ask $who for a new draft…"
            else -> null
        }
    }
