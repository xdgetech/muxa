import MuxaProtocol
import MuxaSession
import SwiftUI

private struct RecoveryChoice {
    let record: TaskRecord
    let operation: String
    let action: RestartRecoveryAction
}

struct RestartRecoverySection: View {
    @Environment(AppModel.self) private var model
    let record: TaskRecord
    @State private var choice: RecoveryChoice?
    @State private var saving = false
    @State private var error: String?

    var body: some View {
        if let operation = record.restartOperation {
            Section("Restart needs recovery") {
                Text("Requested / unconfirmed: \(operation.assignment?.displayName ?? "Assignment unrecorded")")
                Text("The interrupted operation must be resolved before restarting again.")
                    .font(.caption).foregroundStyle(.secondary)
                if model.session.canRecoverRestarts {
                    ForEach(operation.recoveryActions, id: \.rawValue) { action in
                        Button(action.title) { choice = RecoveryChoice(record: record, operation: operation.id, action: action) }
                            .disabled(saving)
                            .accessibilityIdentifier("recover-\(action.rawValue)")
                    }
                }
                if operation.recoveryActions.isEmpty || !model.session.canRecoverRestarts {
                    Text("Open Terminal or update the host to inspect this operation.").font(.caption)
                }
            }
            .accessibilityIdentifier("restart-recovery")
            .alert("Resolve the interrupted restart?", isPresented: Binding(get: { choice != nil }, set: { if !$0 { choice = nil } })) {
                if let selected = choice {
                    Button(selected.action.title) { recover(selected) }
                }
                Button("Cancel", role: .cancel) { choice = nil }
            } message: {
                Text(choice?.action.explanation ?? "")
            }
            .alert("Can't Recover the Restart", isPresented: Binding(get: { error != nil }, set: { if !$0 { error = nil } })) {
                Button("OK", role: .cancel) {}
            } message: { Text(error ?? "") }
        }
    }

    private func recover(_ selected: RecoveryChoice) {
        guard !saving else { return }
        choice = nil
        saving = true
        Task {
            defer { saving = false }
            do {
                _ = try await model.session.recoverRestart(selected.record, operation: selected.operation, action: selected.action)
            } catch { self.error = PaneSession.describe(error) }
        }
    }
}
