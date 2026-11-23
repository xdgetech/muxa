import MuxaV2
import SwiftUI

/// Above a talk's panes: the settings its Bot proposes (ADR 0086 decision 4),
/// the same item as in the Inbox. Apply sends `inbox-apply`, which closes
/// both; Not now hides the bar until the talk opens again.
struct ProposalBar: View {
    @Environment(NextModel.self) private var model
    let item: InboxItem
    let proposal: SettingsProposal
    @State private var hidden = false
    @State private var conflict: String?

    var body: some View {
        if !hidden {
            HStack(alignment: .top, spacing: 12) {
                Image(systemName: "sparkles").foregroundStyle(Palette.accent).padding(.top, 2)
                VStack(alignment: .leading, spacing: 4) {
                    Text("\(bot) proposes \(proposal.ops.count == 1 ? "1 settings change" : "\(proposal.ops.count) settings changes")")
                        .fontWeight(.semibold)
                    ForEach(Array(proposal.ops.enumerated()), id: \.offset) { n, op in
                        Text(op.summary)
                            .font(.system(size: 12, design: .monospaced))
                            .foregroundStyle(Palette.textMid)
                            .accessibilityIdentifier("proposal.op.\(n)")
                    }
                    if let conflict {
                        Text(conflict).font(.system(size: 12)).foregroundStyle(Palette.late)
                            .accessibilityIdentifier("proposal.error")
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                Button("Not now") { hidden = true }
                    .buttonStyle(ActionButtonStyle(kind: .secondary, compact: true))
                    .accessibilityIdentifier("proposal.notnow")
                Button { Task { await apply() } } label: {
                    ActionLabel(title: "Apply", key: Shortcut.applyProposal.display, primary: true)
                }
                .buttonStyle(ActionButtonStyle(kind: .primary, compact: true))
                .keyboardShortcut(.return, modifiers: .command)
                .disabled(model.settings.readOnly)
                .accessibilityIdentifier("proposal.apply")
            }
            .font(.system(size: 13))
            .padding(.horizontal, 14)
            .padding(.vertical, 10)
            .background(Palette.accent.opacity(0.08))
            .overlay(alignment: .bottom) { Rectangle().fill(Palette.border).frame(height: 1) }
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier("proposal.bar")
        }
    }

    private var bot: String { item.bot ?? "The Bot" }

    private func apply() async {
        conflict = nil
        let stale = await model.applyProposal(item)
        if stale {
            conflict = "Settings changed since \(bot) proposed this. Ask \(bot) again."
        } else if let error = model.requestError {
            conflict = error
        }
    }
}
