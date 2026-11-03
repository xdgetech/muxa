package tech.xdge.muxa.protocol

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

/** Explicit recovery never retries process replacement or prompt delivery. */
@Serializable
enum class RestartRecoveryAction {
    @SerialName("cancel-prepared") CANCEL_PREPARED,
    @SerialName("finish-replacement") FINISH_REPLACEMENT,
    @SerialName("leave-unconfirmed") LEAVE_UNCONFIRMED,
}

/** Public recovery metadata only. Unknown phases/actions remain non-actionable. */
@Serializable
data class RestartOperation(
    val id: String,
    val phase: String,
    val assignment: WorkerAssignment? = null,
    val actions: List<String> = emptyList(),
)
