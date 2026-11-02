package tech.xdge.muxa.protocol

import kotlinx.serialization.KSerializer
import kotlinx.serialization.Serializable
import kotlinx.serialization.SerializationException
import kotlinx.serialization.descriptors.SerialDescriptor
import kotlinx.serialization.encoding.Decoder
import kotlinx.serialization.encoding.Encoder
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonDecoder
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonEncoder
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.jsonPrimitive

/** `"any"` or a list of exact commands, from the host's `[workspace.loop]`. */
@Serializable(with = WorkspaceChecks.Serializer::class)
sealed interface WorkspaceChecks {
    /** Any command, at most 4096 bytes, with no control character. */
    data object Any : WorkspaceChecks

    /** Only these commands, exactly. */
    data class Only(val commands: List<String>) : WorkspaceChecks

    object Serializer : KSerializer<WorkspaceChecks> {
        override val descriptor: SerialDescriptor = JsonElement.serializer().descriptor

        override fun deserialize(decoder: Decoder): WorkspaceChecks {
            val json = decoder as? JsonDecoder ?: throw SerializationException("checks need JSON")
            return when (val e = json.decodeJsonElement()) {
                is JsonArray -> Only(e.map { it.jsonPrimitive.content })
                is JsonPrimitive ->
                    if (e.isString && e.content == "any") Any else throw SerializationException("checks must be \"any\" or a list")
                else -> throw SerializationException("checks must be \"any\" or a list")
            }
        }

        override fun serialize(encoder: Encoder, value: WorkspaceChecks) {
            val json = encoder as? JsonEncoder ?: throw SerializationException("checks need JSON")
            json.encodeJsonElement(
                when (value) {
                    Any -> JsonPrimitive("any")
                    is Only -> JsonArray(value.commands.map { JsonPrimitive(it) })
                },
            )
        }
    }
}

/** What a client asks for when it starts a Task Loop (ADR 0019). The host fills in the defaults. */
@Serializable
data class LoopSpec(
    /** A shell command, run in the task's `dir`; one of the workspace's `checks`. */
    val check: String,
    /** Failed checks before the loop stops: 1 to 10, default 3. */
    val maxAttempts: Int? = null,
    /** Seconds one check may run: 10 to 3600, default 600. */
    val timeout: Long? = null,
)

/** A task's Task Loop: the host runs [check] after each finished turn of the agent, and types a fail prompt until it passes (ADR 0019). */
@Serializable
data class TaskLoop(
    val check: String,
    val maxAttempts: Int,
    /** Seconds. */
    val timeout: Long,
    val state: LoopState,
    /** Failed checks so far. */
    val attempts: Int,
    /** Epoch seconds: when the loop started or last typed a prompt. */
    val armedAt: Long,
    /** Epoch seconds of the last state change. */
    val since: Long,
    /** Set when the state is `paused` or `stopped`. */
    val reason: LoopReason? = null,
    val lastCheck: LoopCheck? = null,
) {
    /** Paused for the user, or stopped for a reason the user did not give: the Inbox's Needs attention (ADR 0019). */
    val needsUser: Boolean
        get() = when (state) {
            LoopState.PAUSED -> true
            LoopState.STOPPED -> reason != LoopReason.USER && reason != LoopReason.STATUS
            else -> false
        }
}

/** A state added by a newer bridge keeps its name, with no known meaning. */
@Serializable
@JvmInline
value class LoopState(val raw: String) {
    companion object {
        /** Waits for the agent's next finished turn. */
        val ARMED = LoopState("armed")
        val CHECKING = LoopState("checking")

        /** The agent waits for the user; the loop goes on after the answer. */
        val PAUSED = LoopState("paused")
        val PASSED = LoopState("passed")
        val STOPPED = LoopState("stopped")
    }
}

/** Why a loop is `paused` or `stopped`. A reason added by a newer bridge keeps its name. */
@Serializable
@JvmInline
value class LoopReason(val raw: String) {
    companion object {
        /** The agent waits for the user (paused). */
        val WAITING = LoopReason("waiting")

        /** `maxAttempts` checks failed. */
        val LIMIT = LoopReason("limit")

        /** A check ran longer than `timeout`. */
        val TIMEOUT = LoopReason("timeout")

        /** The agent's state is `error`. */
        val ERROR = LoopReason("error")

        /** No linked session, no live agent pane, or a shell in the foreground. */
        val NO_AGENT = LoopReason("no-agent")

        /** A prompt could not be typed within 30 seconds. */
        val NOT_SENT = LoopReason("not-sent")

        /** The agent did not take the typed prompt. */
        val STALLED = LoopReason("stalled")

        /** The user set the status to anything but `open`. */
        val STATUS = LoopReason("status")

        /** The user stopped it. */
        val USER = LoopReason("user")
    }
}

/** The last check a loop ran. */
@Serializable
data class LoopCheck(
    val attempt: Int,
    /** Null when the check timed out. */
    val exit: Int? = null,
    /** How long it ran. */
    val seconds: Long,
    /** Epoch seconds when it ended. */
    val at: Long,
)

/** A prompt the host waits to type into the task's agent, kept in the record so that the wait survives a restart (ADR 0019). */
@Serializable
data class PendingPrompt(
    val kind: PendingPromptKind,
    /** Epoch seconds when the wait began. */
    val since: Long,
    /** The `id` of the run the prompt is for. */
    val run: Long,
)

/** A kind added by a newer bridge keeps its name. */
@Serializable
@JvmInline
value class PendingPromptKind(val raw: String) {
    companion object {
        /** The start prompt of `create-task`. */
        val START = PendingPromptKind("start")

        /** The prompt of `restart-agent`. */
        val RESTART = PendingPromptKind("restart")
    }
}
