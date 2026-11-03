import MuxaProtocol

extension RestartOperation {
    public var recoveryActions: [RestartRecoveryAction] {
        guard !id.isEmpty else { return [] }
        let permitted: [RestartRecoveryAction]
        switch phase {
        case "prepared": permitted = [.cancelPrepared]
        case "dispatching", "delivering": permitted = [.leaveUnconfirmed]
        case "replaced": permitted = [.finishReplacement, .leaveUnconfirmed]
        default: permitted = []
        }
        return permitted.filter { actions.contains($0.rawValue) }
    }

    public var blocksRestart: Bool { true }
}

extension RestartRecoveryAction {
    public var title: String {
        switch self {
        case .cancelPrepared: "Cancel prepared restart"
        case .finishReplacement: "Finish recorded replacement"
        case .leaveUnconfirmed: "Leave unconfirmed"
        }
    }
    public var explanation: String {
        switch self {
        case .cancelPrepared: "Discard the prepared restart. The host verifies that dispatch was never authorized."
        case .finishReplacement: "Reconcile the recorded replacement and finalize its run. This does not restart the agent or send a prompt."
        case .leaveUnconfirmed: "Keep the outcome unconfirmed and stop automatic work. This does not restart the agent or send a prompt."
        }
    }
}
