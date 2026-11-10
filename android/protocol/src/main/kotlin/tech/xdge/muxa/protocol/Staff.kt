package tech.xdge.muxa.protocol

import kotlinx.serialization.Serializable

// Staff and hand-offs inside a workspace (ADR 0051), at `work` level 12. A staff member is a
// host `[[worker]]`; a request version runs as a chain of steps, one run each.

/** One run of a request version's chain. Step 1 is the first member asked. */
@Serializable
data class RequestStep(
    /** From 1. */
    val step: Int,
    /** The staff member who runs it: a host `[[worker]]` name. */
    val worker: String,
    val by: StepBy,
    /** As a request's, for this step's run. */
    val delivery: Delivery,
    /** Epoch seconds: when [delivery] was last derived. */
    val deliveryAt: Long,
    /** The step's run record, `t-…` of kind `assistant-run`. */
    val run: String? = null,
    /** The hand-off this step's result asks for: the next step, once approved. */
    val handoff: Handoff? = null,
)

/** Who picked a step's worker. Unknown values stay as their name: show them as unknown. */
@Serializable
@JvmInline
value class StepBy(val raw: String) {
    companion object {
        /** The user named the worker. */
        val USER = StepBy("user")

        /** The overall assistant named it in a `delegate` proposal. */
        val OVERALL = StepBy("overall")

        /** Nobody named one: the workspace's lead took it. */
        val LEAD = StepBy("lead")

        /** The previous step handed it on. */
        val HANDOFF = StepBy("handoff")
    }
}

/** "Pass this to that member next." */
@Serializable
data class Handoff(
    /** A staff member of the same workspace, not yet in the chain. */
    val worker: String,
    /** What the next member does within the request's scope, 1 byte to 1 KiB. Never logged. */
    val purpose: String,
)

/** A `handoff` Inbox item's payload: what approve starts. */
@Serializable
data class HandoffProposal(
    /** `q-…`. */
    val request: String,
    val version: Long,
    /** The step whose result asks for the hand-off; approve starts `step + 1`. */
    val step: Int,
    /** The Work item, to open its detail. */
    val work: String,
    /** The worker of [step]. */
    val from: String,
    /** The next worker. */
    val worker: String,
    /** Never logged. */
    val purpose: String,
)

/** A workspace's staff member, in `inbox-list`'s `assistant.delegates[]`. */
@Serializable
data class StaffMember(
    /** A host `[[worker]]` name. */
    val worker: String,
    /** The worker's job description, at most 1 KiB. */
    val description: String,
    /** The member who takes requests that name no worker. */
    val lead: Boolean = false,
    /** Epoch seconds: when the member's own notes last changed. Null: none. */
    val notesAt: Long? = null,
)
