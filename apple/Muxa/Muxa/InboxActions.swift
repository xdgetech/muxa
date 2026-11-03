import MuxaProtocol
import MuxaSession
import SwiftUI

/// What an Inbox action asks the user before it goes to the host.
enum InboxPrompt: Identifiable {
    /// The Later choices for a row.
    case later(InboxRow)
    /// A time picked by hand.
    case pickTime(InboxRow)
    /// The proposal's action, target and content, before approval.
    case approve(InboxEntry)

    var id: String {
        switch self {
        case .later(let row): "later-\(row.id)"
        case .pickTime(let row): "pick-\(row.id)"
        case .approve(let entry): "approve-\(entry.id)"
        }
    }
}

/// Runs the Inbox actions and asks first where an action needs a choice. Shared by
/// the Inbox list and an entry's detail. `approved` gets the Task approval created.
struct InboxActionsModifier: ViewModifier {
    @Environment(AppModel.self) private var model
    @Binding var prompt: InboxPrompt?
    var approved: (String) -> Void
    @State private var error: String?

    func body(content: Content) -> some View {
        content
            .confirmationDialog(
                "Show Again",
                isPresented: Binding(get: { laterRow != nil }, set: { if !$0, laterRow != nil { prompt = nil } }),
                titleVisibility: .visible,
                presenting: laterRow
            ) { row in
                ForEach(LaterChoice.allCases, id: \.title) { choice in
                    Button(choice.title) { run { try await model.later(row, until: choice.until()) } }
                }
                Button("Pick a Time…") { DispatchQueue.main.async { prompt = .pickTime(row) } }
                Button("Cancel", role: .cancel) {}
            } message: { _ in
                Text("It leaves the Inbox until then, on every device.")
            }
            .sheet(item: Binding(get: { sheetPrompt }, set: { if $0 == nil { prompt = nil } })) { prompt in
                switch prompt {
                case .pickTime(let row):
                    LaterPicker { until in run { try await model.later(row, until: until) } }
                case .approve(let entry):
                    ApproveSheet(entry: entry) {
                        run {
                            if let task = try await model.approve(entry) { approved(task) }
                        }
                    }
                case .later:
                    EmptyView()
                }
            }
            .alert("Can't Update the Inbox", isPresented: Binding(get: { error != nil }, set: { if !$0 { error = nil } })) {
                Button("OK", role: .cancel) {}
            } message: {
                Text(error ?? "")
            }
    }

    private var laterRow: InboxRow? {
        if case .later(let row) = prompt { return row }
        return nil
    }

    private var sheetPrompt: InboxPrompt? {
        if case .later = prompt { return nil }
        return prompt
    }

    private func run(_ action: @escaping () async throws -> Void) {
        prompt = nil
        Task {
            do { try await action() } catch { self.error = PaneSession.describe(error) }
        }
    }
}

extension View {
    func inboxActions(_ prompt: Binding<InboxPrompt?>, approved: @escaping (String) -> Void) -> some View {
        modifier(InboxActionsModifier(prompt: prompt, approved: approved))
    }
}

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
    let entry: InboxEntry
    let approve: () -> Void
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            Form {
                if let p = entry.proposal {
                    Section("Action") { Text(p.action).accessibilityIdentifier("approve-action") }
                    Section("Target") { Text(p.target).textSelection(.enabled).accessibilityIdentifier("approve-target") }
                    Section {
                        Text(p.content).font(.callout.monospaced()).textSelection(.enabled)
                            .accessibilityIdentifier("approve-content")
                    } header: {
                        Text("Content")
                    } footer: {
                        Text("Approve creates one Task in \(entry.workspace) that performs this action with this content, unchanged. The agent's own tools decide what it can do; muxa does not check the result.")
                    }
                }
            }
            .navigationTitle(entry.title)
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
