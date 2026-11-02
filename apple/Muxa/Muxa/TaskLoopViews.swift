import MuxaProtocol
import MuxaSession
import SwiftUI

// The Task Loop in the Tasks sheet (ADR 0019): the detail's Loop section, the Start
// loop sheet, and the Inbox row's badge. Labels match the Android app.

/// The task's loop as its record says, and the buttons that start or stop it.
struct LoopSection: View {
    @Environment(AppModel.self) private var model
    let item: InboxItem
    @Binding var starting: LoopStart?
    @Binding var error: String?
    @State private var stopping = false

    var body: some View {
        let r = item.record
        let loop = r.loop
        let checks = model.checks(of: r.workspace)
        // A state this app does not know may still be running: offer both.
        let unknown = loop.map { !$0.isKnownState } ?? false
        Section("Loop") {
            if let loop { LoopFacts(loop: loop) } else { Text("This Task has no loop.") }
            if !model.canLoop {
                Text(loopNeedsDaemon)
                    .font(.footnote).foregroundStyle(.secondary)
                    .accessibilityIdentifier("loop-needs-daemon")
            } else if checks == nil, loop?.isActive != true {
                Text(LoopDraft.noChecks)
                    .font(.footnote).foregroundStyle(.secondary)
                    .accessibilityIdentifier("loop-no-checks")
            }
            if model.canLoop, let checks, loop?.isActive != true || unknown {
                Button("Start loop", systemImage: "play") { starting = LoopStart(record: r, checks: checks) }
                    .accessibilityIdentifier("loop-start")
            }
            if model.canLoop, loop != nil, loop?.isActive == true || unknown {
                Button("Stop loop", systemImage: "stop") { stop(r) }
                    .disabled(stopping)
                    .accessibilityIdentifier("loop-stop")
            }
            // The daemon writes CHECK.md after each check; the Reader reads it from the task's session.
            if loop?.lastCheck != nil, model.session.canReadDocs, item.session != nil {
                Button("Open check log", systemImage: Icon.document) { model.open(item, as: .checkFile, returnToDetail: true) }
                    .accessibilityIdentifier("loop-open-log")
            }
        }
    }

    private func stop(_ r: TaskRecord) {
        guard !stopping else { return }
        stopping = true
        Task {
            defer { stopping = false }
            do {
                _ = try await model.session.stopLoop(r)
            } catch {
                self.error = PaneSession.describe(error)
            }
        }
    }
}

private struct LoopFacts: View {
    let loop: TaskLoop

    var body: some View {
        HStack {
            Text(loop.stateTitle).fontWeight(.semibold).accessibilityIdentifier("loop-state")
            Spacer()
            Text("Attempts \(loop.attemptsLabel)")
                .font(.callout).foregroundStyle(.secondary)
                .accessibilityIdentifier("loop-attempts")
        }
        Text(loop.explanation)
            .foregroundStyle(loop.needsUser ? Theme.failure : .primary)
            .accessibilityIdentifier("loop-reason")
        VStack(alignment: .leading, spacing: 2) {
            Text("Check").font(.caption).foregroundStyle(.secondary)
            Text(loop.check)
                .font(.callout.monospaced())
                .textSelection(.enabled)
                .accessibilityIdentifier("loop-check-command")
        }
        if let check = loop.lastCheck {
            VStack(alignment: .leading, spacing: 2) {
                Text("Last check").font(.caption).foregroundStyle(.secondary)
                Text(check.line(now: UInt64(Date().timeIntervalSince1970)))
                    .font(.callout)
                    .accessibilityIdentifier("loop-last-check")
            }
        }
    }
}

/// The Start loop sheet's task, and the loop it edits.
struct LoopStart: Identifiable {
    var record: TaskRecord
    var draft: LoopDraft

    var id: String { record.id }

    /// Starts with the task's last check when the workspace still allows it.
    init(record: TaskRecord, checks: WorkspaceChecks) {
        self.record = record
        let fresh = LoopDraft(checks: checks)
        if let last = record.loop, LoopDraft(checks: checks, check: last.check).problem == nil {
            draft = LoopDraft(checks: checks, check: last.check, maxAttempts: Int(last.maxAttempts))
        } else {
            draft = fresh
        }
    }
}

extension View {
    /// The Start loop sheet and the loop's error alert, for a task detail.
    func loopSheets(starting: Binding<LoopStart?>, error: Binding<String?>) -> some View {
        modifier(LoopSheets(starting: starting, error: error))
    }
}

private struct LoopSheets: ViewModifier {
    @Environment(AppModel.self) private var model
    @Binding var starting: LoopStart?
    @Binding var error: String?

    func body(content: Content) -> some View {
        content
            .sheet(item: $starting) { start in
                StartLoopSheet(record: start.record, draft: start.draft) { draft, reset in
                    starting = nil
                    begin(start.record, draft, reset: reset)
                }
            }
            .alert("Can't Change the Loop", isPresented: Binding(get: { error != nil }, set: { if !$0 { error = nil } })) {
                Button("OK", role: .cancel) {}
            } message: {
                Text(error ?? "")
            }
    }

    private func begin(_ record: TaskRecord, _ draft: LoopDraft, reset: Bool) {
        guard let spec = draft.spec else { return }
        Task {
            do {
                _ = try await model.session.startLoop(record, loop: spec, reset: reset)
            } catch {
                self.error = PaneSession.describe(error)
            }
        }
    }
}

/// Asks for the check and max attempts, then starts the loop.
private struct StartLoopSheet: View {
    @Environment(\.dismiss) private var dismiss
    let record: TaskRecord
    @State var draft: LoopDraft
    let start: (LoopDraft, Bool) -> Void
    @State private var reset = false

    static let explained =
        "The host runs the check after each of the agent's turns. When it fails, the agent is asked to fix it, until the check passes or the attempts run out."

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    Text(Self.explained).font(.callout).foregroundStyle(.secondary)
                }
                Section {
                    LoopFields(draft: $draft)
                    if (record.loop?.attempts ?? 0) > 0 {
                        Toggle("Count attempts from zero", isOn: $reset)
                            .accessibilityIdentifier("loop-reset")
                    }
                }
                if let problem = draft.problem, draft.checks != .any || !draft.check.isEmpty {
                    Section {
                        Label(problem, systemImage: Icon.warning)
                            .foregroundStyle(Theme.failure)
                            .accessibilityIdentifier("loop-problem")
                    }
                }
            }
            .navigationTitle("Start the loop")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }.keyboardShortcut(.cancelAction)
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Start") { start(draft, reset) }
                        .disabled(draft.problem != nil)
                        .accessibilityIdentifier("loop-start-confirm")
                }
            }
        }
    }
}

/// The check, from the workspace's list or typed for `"any"`, and the max attempts.
struct LoopFields: View {
    @Binding var draft: LoopDraft

    var body: some View {
        switch draft.checks {
        case nil:
            Text(LoopDraft.noChecks).font(.footnote).foregroundStyle(.secondary)
        case .any:
            LabeledContent("Check command") {
                TextField("Check command", text: $draft.check, prompt: Text("make test"))
                    .font(.body.monospaced())
                    .multilineTextAlignment(.trailing)
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()
                    .accessibilityIdentifier("loop-check")
            }
        case .list(let commands):
            Picker("Check", selection: $draft.check) {
                ForEach(commands, id: \.self) { command in
                    Text(command).font(.body.monospaced()).tag(command)
                }
            }
            .pickerStyle(.menu)
            .accessibilityIdentifier("loop-check")
        }
        if draft.checks != nil {
            Picker("Max attempts", selection: $draft.maxAttempts) {
                ForEach(LoopDraft.attempts, id: \.self) { n in
                    Text("\(n)").tag(n)
                }
            }
            .pickerStyle(.menu)
            .accessibilityIdentifier("loop-max-attempts")
        }
    }
}

/// The New Task sheet's switch, and the loop's fields when it is on.
struct NewTaskLoop: View {
    @Environment(AppModel.self) private var model
    @Binding var draft: TaskRecordDraft

    var body: some View {
        let checks = model.checks(of: draft.workspace)
        Section {
            if !model.canLoop {
                Text(loopNeedsDaemon).font(.footnote).foregroundStyle(.secondary)
            } else {
                Toggle("Run a check after each turn", isOn: Binding(
                    get: { draft.loop != nil },
                    set: { draft.loop = $0 ? LoopDraft(checks: checks) : nil }))
                    .disabled(checks == nil)
                    .accessibilityIdentifier("record-with-loop")
                if checks == nil {
                    Text(LoopDraft.noChecks).font(.footnote).foregroundStyle(.secondary)
                } else if let loop = draft.loop {
                    LoopFields(draft: Binding(get: { draft.loop ?? loop }, set: { draft.loop = $0 }))
                }
            }
        }
    }
}

/// The Inbox row's loop badge; in the failure color when the loop needs the user.
struct LoopBadge: View {
    let record: TaskRecord

    var body: some View {
        if let loop = record.loop {
            Text(loop.badge)
                .font(.caption)
                .foregroundStyle(loop.needsUser ? Theme.failure : .secondary)
                .accessibilityIdentifier("loop-badge-\(record.id)")
        }
    }
}
