import MuxaProtocol
import MuxaSession
import SwiftUI

/// Who answers a request (ADR 0051 §7): "Let <lead> decide", then each staff member with
/// its description. Shown only for a workspace with more than one member.
struct StaffPicker: View {
    let staff: [StaffMember]
    @Binding var selection: String?
    /// The identifier prefix of the rows: `work-ask-staff` or `capture-staff`.
    let id: String

    private var lead: String { (staff.first(where: \.lead) ?? staff.first)?.worker ?? "the lead" }

    var body: some View {
        Section {
            row(title: "Let \(lead) decide", subtitle: nil, worker: nil)
            ForEach(staff, id: \.worker) { member in
                row(title: member.worker, subtitle: member.description, worker: member.worker)
            }
        } header: {
            Label("Staff Member", systemImage: Icon.staff)
        }
    }

    private func row(title: String, subtitle: String?, worker: String?) -> some View {
        Button { selection = worker } label: {
            HStack {
                VStack(alignment: .leading, spacing: 2) {
                    Text(title).foregroundStyle(.primary)
                    if let subtitle, !subtitle.isEmpty {
                        Text(subtitle).font(.caption).foregroundStyle(.secondary).lineLimit(3)
                    }
                }
                Spacer(minLength: 8)
                if selection == worker {
                    Image(systemName: "checkmark").foregroundStyle(Theme.accent).accessibilityHidden(true)
                }
            }
            .contentShape(Rectangle())
        }
        .accessibilityIdentifier("\(id)-\(worker ?? "lead")")
        .accessibilityAddTraits(selection == worker ? .isSelected : [])
    }
}

/// A request's chain on a level 12 host (ADR 0051 §7): "a → b → c", then each step with
/// its delivery and its Result, then a hand-off that waits, greyed, with Approve and
/// Reject. Older versions' Results stay under Earlier.
struct ChainSections: View {
    let request: AssistantRequest

    var body: some View {
        Section {
            Text(request.chainShown).accessibilityIdentifier("work-chain")
        } header: {
            Label("Staff", systemImage: Icon.staff)
        }
        ForEach(request.steps, id: \.step) { step in
            StepSection(step: step, handedOn: request.steps.contains { $0.step == step.step + 1 })
            if let result = request.result(ofStep: step.step) {
                ResultBody(request: request, result: result, header: "Result · \(step.worker)", tag: "step-\(step.step)")
            }
        }
        if let waiting = request.waitingHandoff {
            WaitingHandoffSection(request: request, from: waiting.step, handoff: waiting.handoff)
        }
        EarlierResults(request: request)
    }
}

/// One step: who picked its member, and its run's delivery with its age.
private struct StepSection: View {
    let step: RequestStep
    /// The next step started: the hand-off was approved or ran by itself.
    let handedOn: Bool

    var body: some View {
        Section {
            LabeledContent("Picked By", value: step.by.shown)
            LabeledContent("Delivery", value: "\(step.delivery.shown) · \(step.deliveryAt.shownAge)")
                .accessibilityIdentifier("work-step-delivery-\(step.step)")
            if let advice = step.delivery.advice {
                Label(advice, systemImage: Icon.warning).font(.callout).foregroundStyle(Theme.attention)
            }
            if handedOn, let handoff = step.handoff {
                Label("Handed on to \(handoff.worker): \(handoff.purpose)", systemImage: Icon.handoff)
                    .font(.callout).foregroundStyle(.secondary)
            }
        } header: {
            Text("Step \(step.step) · \(step.worker)").accessibilityIdentifier("work-step-\(step.step)")
        }
    }
}

/// The next step a hand-off asks for, before it starts. Approve and Reject answer its
/// Inbox item; what the item says after that is shown instead.
private struct WaitingHandoffSection: View {
    @Environment(AppModel.self) private var model
    let request: AssistantRequest
    let from: RequestStep
    let handoff: Handoff
    @State private var sending = false
    @State private var error: String?

    var body: some View {
        let items = model.session.handoffItems(for: request, step: from.step)
        let waiting = items.first { $0.state == .open || $0.state == .later }
        Section {
            // Greyed: the step has not started.
            Text(handoff.purpose).textSelection(.enabled).foregroundStyle(.secondary)
                .accessibilityIdentifier("work-handoff-purpose")
            if let waiting {
                HStack(spacing: 10) {
                    Button("Approve") { send(waiting, .approve) }
                        .buttonStyle(.borderedProminent)
                        .accessibilityIdentifier("work-handoff-approve")
                    Button("Reject") { send(waiting, .reject) }
                        .buttonStyle(.bordered)
                        .accessibilityIdentifier("work-handoff-reject")
                }
                .disabled(sending || !model.isConnected)
            } else {
                Text(outcome(items.first)).font(.callout).foregroundStyle(.secondary)
                    .accessibilityIdentifier("work-handoff-outcome")
            }
            if let error {
                Label(error, systemImage: Icon.warning).foregroundStyle(Theme.failure)
                    .accessibilityIdentifier("work-handoff-error")
            }
        } header: {
            Text("Step \(from.step + 1) · \(handoff.worker)").accessibilityIdentifier("work-step-next")
        } footer: {
            Text("\(from.worker) asks \(handoff.worker) to take it on next.")
        }
    }

    /// What became of the hand-off when nothing waits on the user.
    private func outcome(_ item: InboxItem?) -> String {
        switch (item?.state, item?.answer) {
        case (.answered, .reject): "Rejected. The chain ended with these results."
        case (.answered, .approve): "Approved. The next step starts."
        case (.resolved, _): "The chain ended here."
        default: "Not started."
        }
    }

    private func send(_ item: InboxItem, _ action: InboxAction) {
        sending = true
        error = nil
        Task {
            defer { sending = false }
            error = await model.answerHandoff(item, action)
        }
    }
}

/// What approving a `handoff` item starts (ADR 0051 §4): the next staff member's step.
struct HandoffProposalView: View {
    @Environment(AppModel.self) private var model
    let proposal: HandoffProposal

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            LabeledContent("From", value: proposal.from)
                .accessibilityIdentifier("item-handoff-from")
            LabeledContent("To", value: proposal.worker)
                .accessibilityIdentifier("item-handoff-to")
            if let item = model.workItem(proposal.work) {
                Button(item.title) { model.select(.work(proposal.work)) }
                    .accessibilityIdentifier("item-handoff-work")
            }
            Text(proposal.purpose).textSelection(.enabled)
                .accessibilityIdentifier("item-handoff-purpose")
            Text("Approve starts step \(proposal.step + 1) with \(proposal.worker). "
                 + "Reject ends the chain with the results it has.")
                .font(.caption).foregroundStyle(.secondary)
        }
    }
}

extension HandoffProposal {
    /// "researcher → writer".
    var shown: String { "\(from) → \(worker)" }
}
