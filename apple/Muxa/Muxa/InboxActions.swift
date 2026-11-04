import MuxaProtocol
import MuxaSession
import SwiftUI

/// Later at a time the user picks.
struct LaterPicker: View {
    let save: (UInt64) -> Void
    @Environment(\.dismiss) private var dismiss
    @State private var date = Date().addingTimeInterval(3 * 3600)

    var body: some View {
        NavigationStack {
            Form {
                DatePicker("Show again", selection: $date, in: Date()..., displayedComponents: [.date, .hourAndMinute])
                    .datePickerStyle(.graphical)
                    .accessibilityIdentifier("later-date")
                if let problem = laterProblem(date) {
                    Text(problem).foregroundStyle(Theme.failure)
                }
            }
            .navigationTitle("Later")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Save") { save(UInt64(date.timeIntervalSince1970)) }
                        .disabled(laterProblem(date) != nil)
                        .accessibilityIdentifier("later-save")
                }
            }
        }
    }
}

/// The proposal exactly as the Task will get it. Approval creates one Task.
struct ApproveSheet: View {
    let title: String
    let workspace: String
    let proposal: Proposal?
    let approve: () -> Void
    @Environment(\.dismiss) private var dismiss

    init(title: String, workspace: String, proposal: Proposal?, approve: @escaping () -> Void) {
        self.title = title
        self.workspace = workspace
        self.proposal = proposal
        self.approve = approve
    }

    var body: some View {
        NavigationStack {
            Form {
                if let p = proposal {
                    Section("Action") { Text(p.action).accessibilityIdentifier("approve-action") }
                    Section("Target") { Text(p.target).textSelection(.enabled).accessibilityIdentifier("approve-target") }
                    Section {
                        Text(p.content).font(.callout.monospaced()).textSelection(.enabled)
                            .accessibilityIdentifier("approve-content")
                    } header: {
                        Text("Content")
                    } footer: {
                        Text("Approve creates one Task in \(workspace) that performs this action with this content, unchanged. The agent's own tools decide what it can do; muxa does not check the result.")
                    }
                }
            }
            .navigationTitle(title)
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Approve", action: approve).accessibilityIdentifier("approve-confirm")
                }
            }
        }
    }
}
