package tech.xdge.muxa.protocol

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

/** The user's original input (ADR 0048). Its [text] is never rewritten. Never logged. */
@Serializable
data class Capture(
    /** `c-` and 8 hex digits. */
    val id: String,
    val rev: Long,
    /** The client's id for this capture; the host stores one Capture per value. */
    val clientId: String,
    /** At most 16 KiB. */
    val text: String,
    val source: CaptureSource,
    val state: CaptureState,
    /** Epoch seconds. */
    val createdAt: Long,
    /** Epoch seconds. */
    val updatedAt: Long,
    val workspace: String? = null,
    /** The Work items made from it or linked to it. */
    val work: List<String> = emptyList(),
)

/** Where a Capture came from. Unknown sources stay as their name. */
@Serializable
@JvmInline
value class CaptureSource(val raw: String) {
    companion object {
        val APP = CaptureSource("app")
        val TMUX = CaptureSource("tmux")
        val CLI = CaptureSource("cli")
    }
}

/** Unknown states stay as their name. */
@Serializable
@JvmInline
value class CaptureState(val raw: String) {
    companion object {
        /** Saved, not sorted yet: listed under Unsorted. */
        val RECEIVED = CaptureState("received")

        /** Sorted into a Work item, linked to one, or kept as a note. */
        val SORTED = CaptureState("sorted")
    }
}

/** The user's unfinished work (ADR 0048). Only the user completes or withdraws it. */
@Serializable
data class WorkItem(
    /** `w-` and 8 hex digits. */
    val id: String,
    /** Grows by one on every write; `update-work` sends the one the client saw. */
    val rev: Long,
    /** One line, at most 200 characters. */
    val title: String,
    val state: WorkState,
    /** Epoch seconds. */
    val createdAt: Long,
    /** Epoch seconds. */
    val updatedAt: Long,
    val workspace: String? = null,
    /** Set by `later`: listed below the others, as Inbox items (ADR 0040). */
    val laterAt: Long? = null,
    /** With `withdrawn` after `merge`: the Work item it was merged into. */
    val mergedInto: String? = null,
    /** Level 10: set by `pin`. Pinned items are listed first, in pin order (ADR 0049). */
    val pinnedAt: Long? = null,
    /** Level 10: the user's deadline. Only the user sets it. */
    val due: Due? = null,
    val captures: List<String> = emptyList(),
    /** Assistant requests, oldest first. */
    val requests: List<String> = emptyList(),
    /** Tasks started for it, oldest first. */
    val tasks: List<String> = emptyList(),
)

/**
 * `in-progress` and `waiting-decision` are derived by the host when it lists the item.
 * An unknown state keeps its name on the wire; [shown] reads it as [OPEN], as PROTOCOL.md says.
 */
@Serializable
@JvmInline
value class WorkState(val raw: String) {
    val shown: WorkState get() = if (this in KNOWN) this else OPEN

    companion object {
        val OPEN = WorkState("open")
        val IN_PROGRESS = WorkState("in-progress")
        val WAITING_DECISION = WorkState("waiting-decision")
        val DONE = WorkState("done")
        val WITHDRAWN = WorkState("withdrawn")
        private val KNOWN = setOf(OPEN, IN_PROGRESS, WAITING_DECISION, DONE, WITHDRAWN)
    }
}

/** One delegation of a Work item to a workspace's agent. Each [version] runs at most once. */
@Serializable
data class AssistantRequest(
    /** `q-` and 8 hex digits. */
    val id: String,
    val work: String,
    /** Grows by one each time the user asks again. */
    val version: Long,
    val workspace: String,
    val scope: AssistScope,
    val delivery: Delivery,
    /** Epoch seconds: when [delivery] was last derived. */
    val deliveryAt: Long,
    /** Epoch seconds. */
    val createdAt: Long,
    /** Epoch seconds. */
    val updatedAt: Long,
    /** What the user asked for, at most 4096 bytes. Never logged. */
    val purpose: String? = null,
    /** The current version's run record, `t-…` of kind `assistant-run`. */
    val run: String? = null,
    /** Newest first, at most 10. */
    val results: List<AssistResult> = emptyList(),
    /** Level 12: the current version's chain, step 1 first (ADR 0051 §3). [delivery] and [run] are the newest step's. */
    val steps: List<RequestStep> = emptyList(),
)

/** What the assistant is asked to do. Unknown scopes stay as their name. */
@Serializable
@JvmInline
value class AssistScope(val raw: String) {
    companion object {
        val RESEARCH = AssistScope("research")
        val COMPARE = AssistScope("compare")
        val DRAFT = AssistScope("draft")
    }
}

/** Derived from the run record and the adapter, never from the agent's words. An unknown value keeps its name and means unknown. */
@Serializable
@JvmInline
value class Delivery(val raw: String) {
    companion object {
        /** The run exists; the prompt is not typed yet. */
        val PENDING = Delivery("pending")

        /** The prompt was typed and confirmed. */
        val RECEIVED = Delivery("received")

        /** The adapter reports the agent busy. */
        val PROCESSING = Delivery("processing")

        /** A send was interrupted. Never retyped automatically. */
        val UNCONFIRMED = Delivery("unconfirmed")
        val RESULTED = Delivery("resulted")
        val FAILED = Delivery("failed")

        /** No result in the workspace's `run_timeout`. The agent is not stopped. */
        val TIMED_OUT = Delivery("timed-out")
        val CANCEL_REQUESTED = Delivery("cancel-requested")
        val CANCELLED = Delivery("cancelled")
    }
}

/** What a run returned, after the host's check. */
@Serializable
data class AssistResult(
    /** The request version that produced it. */
    val version: Long,
    /** Epoch seconds: when the host stored it. */
    val at: Long,
    /** At most 4096 bytes. Never logged. */
    val summary: String,
    /** An older version's result. */
    val stale: Boolean = false,
    val findings: List<Finding> = emptyList(),
    val openQuestions: List<String> = emptyList(),
    val next: List<String> = emptyList(),
    /** Paths the run wrote, absolute. */
    val files: List<String> = emptyList(),
    val possibleDuplicates: List<PossibleDuplicate> = emptyList(),
    /** Level 10: the delegate's proposed notes (ADR 0049 §1). Never logged. */
    val notes: Notes? = null,
    /** Level 12: the staff member whose step produced it. */
    val worker: String? = null,
    /** Level 12: the step of [version] that produced it. Null: step 1. */
    val step: Int? = null,
)

/** A deadline the user set on a Work item (ADR 0049 §6): epoch seconds, and the IANA zone it was set in. */
@Serializable
data class Due(val at: Long, val timezone: String)

/**
 * A run's proposed replacement of the workspace's `.muxa/assistant/NOTES.md`. `apply-notes` writes it only
 * while the live file still hashes to [base]: lowercase hex SHA-256 of the file the run read, of no bytes
 * when there was none.
 */
@Serializable
data class Notes(
    /** The full new text, at most 16 KiB. Never logged. */
    val text: String,
    val base: String,
)

@Serializable
data class Finding(
    val text: String,
    /** At least one. */
    val sources: List<ResultSource>,
)

/** A URL or a path, with when the run read it. */
@Serializable
data class ResultSource(
    val url: String? = null,
    val path: String? = null,
    val title: String? = null,
    /** Epoch seconds. */
    val readAt: Long? = null,
)

@Serializable
data class PossibleDuplicate(val work: String, val reason: String)

/** `capture {assist}`: ask the assistant right away. [worker] (level 12) is a staff member; null asks the lead. */
@Serializable
data class AssistSpec(val scope: AssistScope, val purpose: String? = null, val worker: String? = null)

/** How `sort-capture` sorts a Capture. */
@Serializable
enum class SortAction {
    /** Make a new Work item from the Capture. */
    @SerialName("work") WORK,

    /** Link the Capture to an existing Work item. */
    @SerialName("link") LINK,

    /** Keep the Capture as a note; no Work item. */
    @SerialName("note") NOTE,
}

/** A change in `update-work`. */
@Serializable
enum class WorkAction {
    /** Change `title`, `workspace` or, at level 10, `due`. */
    @SerialName("edit") EDIT,
    @SerialName("later") LATER,
    @SerialName("complete") COMPLETE,
    @SerialName("withdraw") WITHDRAW,
    @SerialName("reopen") REOPEN,

    /** Withdraw this item into `into`; its Captures move there. */
    @SerialName("merge") MERGE,

    /** Level 10: list it first, after the items pinned before it. */
    @SerialName("pin") PIN,

    /** Level 10: clear `pinnedAt`. */
    @SerialName("unpin") UNPIN,
}
