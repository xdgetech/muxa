import MuxaProtocol
import MuxaSession
import SwiftUI

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
