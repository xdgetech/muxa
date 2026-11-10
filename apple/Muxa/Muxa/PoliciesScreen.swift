import MuxaProtocol
import MuxaSession
import SwiftUI

/// The Policies sheet from Today: the one place that shows what the overall assistant
/// remembers (ADR 0050 §7).
struct PoliciesScreen: View {
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            PoliciesList()
                .toolbar {
                    ToolbarItem(placement: .confirmationAction) {
                        Button("Done") { dismiss() }.accessibilityIdentifier("policies-done")
                    }
                }
        }
    }
}

/// Active and expired policies with Add, Edit and Delete; hypotheses with Confirm,
/// Reject and Forget; the recent corrections with Forget. Settings pushes it too.
struct PoliciesList: View {
    @Environment(AppModel.self) private var model
    @State private var editing: EditedPolicy?
    @State private var rejecting: InboxItem?
    @State private var forgetting: String?
    @State private var working = false
    @State private var error: String?

    var body: some View {
        List {
            if let error {
                Label(error, systemImage: Icon.warning).foregroundStyle(Theme.failure)
                    .accessibilityIdentifier("policies-error")
            }
            policies
            expired
            hypotheses
            corrections
        }
        .accessibilityIdentifier("policies-list")
        .navigationTitle("Policies")
        .navigationBarTitleDisplayMode(.inline)
        .disabled(working)
        .refreshable { await model.session.loadWork() }
        .sheet(item: $editing) { edited in PolicyEditor(policy: edited.policy) }
        .rejectReason(item: $rejecting) { item, reason in run { await model.reject(item, reason: reason) } }
        .forgetConfirmation(record: $forgetting) { record in run { await model.forget(record) } }
    }

    private var policies: some View {
        Section {
            ForEach(model.session.activePolicies()) { policy in
                PolicyRow(policy: policy) { editing = EditedPolicy(policy: policy) }
                    .swipeActions {
                        Button("Delete", systemImage: Icon.delete, role: .destructive) { delete(policy) }
                    }
                    .contextMenu {
                        Button("Edit", systemImage: Icon.edit) { editing = EditedPolicy(policy: policy) }
                        Button("Delete", systemImage: Icon.delete, role: .destructive) { delete(policy) }
                    }
            }
            Button("Add Policy", systemImage: Icon.add) { editing = EditedPolicy(policy: nil) }
                .disabled(!model.isConnected)
                .accessibilityIdentifier("policy-add")
        } header: {
            Text("Policies")
        } footer: {
            Text("Your own words. A plan follows them over anything the assistant guessed.")
        }
    }

    @ViewBuilder
    private var expired: some View {
        let expired = model.session.expiredPolicies()
        if !expired.isEmpty {
            Section("Expired") {
                ForEach(expired) { policy in
                    PolicyRow(policy: policy) { editing = EditedPolicy(policy: policy) }
                        .foregroundStyle(.secondary)
                        .swipeActions {
                            Button("Delete", systemImage: Icon.delete, role: .destructive) { delete(policy) }
                        }
                }
            }
        }
    }

    @ViewBuilder
    private var hypotheses: some View {
        let hypotheses = model.session.hypotheses
        if !hypotheses.isEmpty {
            Section {
                ForEach(hypotheses) { hypothesis in
                    HypothesisRow(
                        hypothesis: hypothesis, item: model.session.item(for: hypothesis),
                        confirm: { item in run { await model.confirm(item) } },
                        reject: { rejecting = $0 },
                        forget: { forgetting = hypothesis.id })
                }
            } header: {
                Text("Hypotheses")
            } footer: {
                Text("The assistant's guesses. Only one you confirm becomes a policy.")
            }
        }
    }

    @ViewBuilder
    private var corrections: some View {
        let corrections = model.session.corrections
        if !corrections.isEmpty {
            Section {
                ForEach(corrections) { correction in
                    HStack(alignment: .firstTextBaseline) {
                        VStack(alignment: .leading, spacing: 2) {
                            Text(model.shown(correction))
                            // A fixed age: a ticking one keeps the list changing every second.
                            Text(Self.age.localizedString(for: Date(timeIntervalSince1970: TimeInterval(correction.at)),
                                                          relativeTo: Date()))
                                .font(.caption).foregroundStyle(.secondary)
                        }
                        Spacer(minLength: 8)
                        Button("Forget") { forgetting = correction.id }
                            .buttonStyle(.borderless)
                            .accessibilityIdentifier("correction-forget-\(correction.id)")
                    }
                    .accessibilityElement(children: .contain)
                    .accessibilityIdentifier("correction-\(correction.id)")
                }
            } header: {
                Text("Recent Corrections")
            } footer: {
                Text("What you changed on a plan. Later means not now, never unimportant.")
            }
        }
    }

    private static let age: RelativeDateTimeFormatter = {
        let f = RelativeDateTimeFormatter()
        f.unitsStyle = .short
        return f
    }()

    private func delete(_ policy: Policy) {
        run { await model.deletePolicy(policy) }
    }

    private func run(_ body: @escaping () async -> String?) {
        working = true
        error = nil
        Task {
            defer { working = false }
            error = await body()
        }
    }
}

/// A policy's text, its scope and its period. A tap edits it.
private struct PolicyRow: View {
    @Environment(AppModel.self) private var model
    let policy: Policy
    var edit: () -> Void

    var body: some View {
        Button(action: edit) {
            VStack(alignment: .leading, spacing: 2) {
                Text(policy.text)
                Text("\(model.scopeShown(policy.scope)) · \(policy.period)").font(.caption).foregroundStyle(.secondary)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .contentShape(Rectangle())
        }
        .foregroundStyle(.primary)
        .accessibilityIdentifier("policy-\(policy.id)")
    }
}

/// A hypothesis: Confirm and Reject while its Inbox item is open, Forget always.
private struct HypothesisRow: View {
    @Environment(AppModel.self) private var model
    let hypothesis: Hypothesis
    let item: InboxItem?
    var confirm: (InboxItem) -> Void
    var reject: (InboxItem) -> Void
    var forget: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(hypothesis.text)
            Text(details).font(.caption).foregroundStyle(.secondary)
            HStack(spacing: 16) {
                if let item, hypothesis.state == .proposed {
                    Button("Confirm") { confirm(item) }
                        .accessibilityIdentifier("hypothesis-confirm-\(hypothesis.id)")
                    Button("Reject") { reject(item) }
                        .accessibilityIdentifier("hypothesis-reject-\(hypothesis.id)")
                }
                Button("Forget", action: forget)
                    .accessibilityIdentifier("hypothesis-forget-\(hypothesis.id)")
            }
            .buttonStyle(.borderless)
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("hypothesis-\(hypothesis.id)")
    }

    private var details: String {
        let state = hypothesis.state == .rejected ? "Rejected" : hypothesis.confidence.shown
        return "\(model.scopeShown(hypothesis.scope)) · \(state) · \(Correction.basedOn(hypothesis.evidence))"
    }
}

/// The policy a sheet edits; nil adds one.
struct EditedPolicy: Identifiable {
    var policy: Policy?
    var id: String { policy?.id ?? "new" }
}

/// Add or edit a policy: its text, its scope (never the overall workspace), a start, and
/// an end: none, this week, or a date.
struct PolicyEditor: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    let policy: Policy?
    @State private var draft = PolicyDraft()
    @State private var saving = false
    @State private var error: String?

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    TextField("This week, recruiting comes first.", text: $draft.text, axis: .vertical)
                        .lineLimit(2...6)
                        .accessibilityIdentifier("policy-text")
                } footer: {
                    Text("In your own words. The assistant cites it when it follows it.")
                }
                Section {
                    Picker("Applies To", selection: $draft.scope) {
                        Text("All workspaces").tag("all")
                        ForEach(model.session.pickableWorkspaces.map(\.name), id: \.self) { Text($0).tag($0) }
                    }
                    .accessibilityIdentifier("policy-scope")
                    Toggle("Starts Later", isOn: $draft.startsLater).accessibilityIdentifier("policy-starts-later")
                    if draft.startsLater {
                        DatePicker("Starts", selection: $draft.start, displayedComponents: .date)
                    }
                    Picker("Ends", selection: $draft.end) {
                        Text("No end").tag(PolicyDraft.End.none)
                        Text("This week").tag(PolicyDraft.End.thisWeek)
                        Text("On a date").tag(PolicyDraft.End.until)
                    }
                    .accessibilityIdentifier("policy-end")
                    if draft.end == .until {
                        DatePicker("Until", selection: $draft.endDate, in: Date()..., displayedComponents: .date)
                    }
                }
                if let problem = shownProblem {
                    Label(problem, systemImage: Icon.warning).foregroundStyle(Theme.failure)
                        .accessibilityIdentifier("policy-error")
                }
                if let policy {
                    Section {
                        Button("Delete Policy", systemImage: Icon.delete, role: .destructive) { delete(policy) }
                            .accessibilityIdentifier("policy-delete")
                    }
                }
            }
            .navigationTitle(policy == nil ? "New Policy" : "Edit Policy")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }.accessibilityIdentifier("policy-cancel")
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Save", action: save)
                        .disabled(saving || !model.isConnected || draft.problem(editing: policy) != nil)
                        .accessibilityIdentifier("policy-save")
                }
            }
            .onAppear { if let policy { draft = PolicyDraft(policy) } }
        }
    }

    /// The host's refusal, else what keeps Save off once there is text.
    private var shownProblem: String? {
        if let error { return error }
        return draft.text.isEmpty ? nil : draft.problem(editing: policy)
    }

    private func save() {
        saving = true
        error = nil
        Task {
            defer { saving = false }
            let refusal = if let policy { await model.editPolicy(policy, draft) } else { await model.addPolicy(draft) }
            if let refusal { error = refusal } else { dismiss() }
        }
    }

    private func delete(_ policy: Policy) {
        saving = true
        Task {
            defer { saving = false }
            if let refusal = await model.deletePolicy(policy) { error = refusal } else { dismiss() }
        }
    }
}

extension View {
    /// Reject with an optional reason: the host records it with the correction, and
    /// nothing asks for one (ADR 0050 §2).
    func rejectReason(item: Binding<InboxItem?>, reject: @escaping (InboxItem, String) -> Void) -> some View {
        modifier(ReasonPrompt(
            title: "Reject", button: "Reject", subject: item, identifier: "reject",
            message: "Why, if you want to say. The assistant reads it next time.", act: reject))
    }

    /// Forget says what it leaves: plans made before keep their reasons (ADR 0050 §5).
    func forgetConfirmation(record: Binding<String?>, forget: @escaping (String) -> Void) -> some View {
        confirmationDialog(
            "Forget it?", isPresented: Binding(get: { record.wrappedValue != nil }, set: { if !$0 { record.wrappedValue = nil } }),
            titleVisibility: .visible, presenting: record.wrappedValue
        ) { id in
            Button("Forget", role: .destructive) { forget(id) }.accessibilityIdentifier("forget-confirm")
            Button("Cancel", role: .cancel) {}
        } message: { _ in
            Text(PaneSession.forgetNote)
        }
    }
}

/// An alert with one optional text field: Not today and Reject.
struct ReasonPrompt<Subject>: ViewModifier {
    let title: String
    let button: String
    @Binding var subject: Subject?
    let identifier: String
    let message: String
    var act: (Subject, String) -> Void
    @State private var reason = ""

    func body(content: Content) -> some View {
        content.alert(
            title, isPresented: Binding(get: { subject != nil }, set: { if !$0 { subject = nil } }),
            presenting: subject
        ) { subject in
            TextField("Reason (optional)", text: $reason).accessibilityIdentifier("\(identifier)-reason")
            Button(button) {
                act(subject, reason)
                reason = ""
            }
            .accessibilityIdentifier("\(identifier)-confirm")
            Button("Cancel", role: .cancel) { reason = "" }
        } message: { _ in
            Text(message)
        }
    }
}
