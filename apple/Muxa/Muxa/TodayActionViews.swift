import MuxaProtocol
import MuxaSession
import SwiftUI

/// A Today row's buttons at `work` level 17 (ADR 0059 §7): the main action, and Send, which
/// names who gets the text.
struct TodayRowActions: View {
    @Environment(AppModel.self) private var model
    let row: PlanRow

    var body: some View {
        let primary = model.primary(of: row)
        HStack(spacing: 8) {
            Button(primary.label) { model.perform(primary, on: row) }
                .buttonStyle(.borderedProminent)
                .accessibilityIdentifier("today-primary-\(row.target)")
            if let target = model.sendTarget(of: row) {
                Button("Send…", systemImage: Icon.send) { model.todaySend = target }
                    .buttonStyle(.bordered)
                    .accessibilityLabel(target.send.label)
                    .accessibilityIdentifier("today-send-\(row.target)")
            }
        }
        .font(.caption)
        .controlSize(.small)
    }
}

/// The text a Send prompt sends, with the receiver named, and the host's refusal after it.
/// It sits on a list or a screen, not on a row: a row's alert would go when it scrolls away.
struct SendPrompt: ViewModifier {
    @Environment(AppModel.self) private var model
    @Binding var target: SendTarget?
    @State private var text = ""
    @State private var failure: String?

    func body(content: Content) -> some View {
        content
            .alert(
                "Send", isPresented: Binding(get: { target != nil }, set: { if !$0 { target = nil } }),
                presenting: target
            ) { target in
                TextField("Message", text: $text).accessibilityIdentifier("send-text")
                Button("Send") { send(target) }.accessibilityIdentifier("send-confirm")
                Button("Cancel", role: .cancel) { text = "" }
            } message: { target in
                Text(target.send.label)
            }
            .alert("Can't Send", isPresented: Binding(get: { failure != nil }, set: { if !$0 { failure = nil } })) {
                Button("OK", role: .cancel) {}
            } message: {
                Text(failure ?? "")
            }
    }

    private func send(_ target: SendTarget) {
        let sent = text
        text = ""
        Task { failure = await model.send(target, text: sent) }
    }
}

/// The Today rows' Send prompt, on the sidebar's list.
struct TodaySendPrompt: ViewModifier {
    @Environment(AppModel.self) private var model

    func body(content: Content) -> some View {
        @Bindable var model = model
        content.modifier(SendPrompt(target: $model.todaySend))
    }
}

/// A Task the assistant proposes (ADR 0060 §8): title, where it runs, the goal, and that it
/// starts in plan mode. Start Task is the item's main answer.
struct ProposedTaskView: View {
    let proposal: TaskProposal

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text("Proposed Task").font(.footnote.weight(.semibold)).foregroundStyle(.secondary)
            Text(proposal.title).font(.headline).accessibilityIdentifier("item-task-title")
            Text("In \(proposal.workspace) · \(proposal.repo.map { "repo \($0)" } ?? "its own folder")")
                .font(.callout).foregroundStyle(.secondary)
                .accessibilityIdentifier("item-task-where")
            if let worker = proposal.worker {
                Text("Proposed by \(worker)").font(.callout).foregroundStyle(.secondary)
            }
            Text(proposal.goal).textSelection(.enabled).accessibilityIdentifier("item-task-goal")
            Label("Starts in plan mode: its agent plans first, and you approve the plan in the Inbox.",
                  systemImage: Icon.plan)
                .font(.caption)
                .foregroundStyle(.secondary)
                .accessibilityIdentifier("item-task-plan-mode")
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(12)
        .background(.quaternary.opacity(0.5), in: RoundedRectangle(cornerRadius: 10))
    }
}
