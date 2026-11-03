package tech.xdge.muxa.session

import tech.xdge.muxa.protocol.RestartOperation
import tech.xdge.muxa.protocol.RestartRecoveryAction

val RestartOperation.recoveryActions: List<RestartRecoveryAction>
    get() {
        if (id.isEmpty()) return emptyList()
        val permitted = when (phase) {
            "prepared" -> listOf(RestartRecoveryAction.CANCEL_PREPARED)
            "dispatching", "delivering" -> listOf(RestartRecoveryAction.LEAVE_UNCONFIRMED)
            "replaced" -> listOf(RestartRecoveryAction.FINISH_REPLACEMENT, RestartRecoveryAction.LEAVE_UNCONFIRMED)
            else -> emptyList()
        }
        return permitted.filter { action -> action.wireName in actions }
    }

val RestartOperation.blocksRestart: Boolean get() = true

val RestartRecoveryAction.wireName: String get() = when (this) {
    RestartRecoveryAction.CANCEL_PREPARED -> "cancel-prepared"
    RestartRecoveryAction.FINISH_REPLACEMENT -> "finish-replacement"
    RestartRecoveryAction.LEAVE_UNCONFIRMED -> "leave-unconfirmed"
}
val RestartRecoveryAction.title: String get() = when (this) {
    RestartRecoveryAction.CANCEL_PREPARED -> "Cancel prepared restart"
    RestartRecoveryAction.FINISH_REPLACEMENT -> "Finish recorded replacement"
    RestartRecoveryAction.LEAVE_UNCONFIRMED -> "Leave unconfirmed"
}
val RestartRecoveryAction.explanation: String get() = when (this) {
    RestartRecoveryAction.CANCEL_PREPARED -> "Discard the prepared restart. The host verifies that dispatch was never authorized."
    RestartRecoveryAction.FINISH_REPLACEMENT -> "Reconcile the recorded replacement and finalize its run. This does not restart the agent or send a prompt."
    RestartRecoveryAction.LEAVE_UNCONFIRMED -> "Keep the outcome unconfirmed and stop automatic work. This does not restart the agent or send a prompt."
}
