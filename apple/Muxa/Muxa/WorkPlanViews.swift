import MuxaProtocol
import MuxaSession
import SwiftUI

/// Why the current plan lists this Work item in Today (ADR 0047 §8).
struct PlanReasonSection: View {
    let row: PlanRow

    var body: some View {
        Section("Today") {
            Text(row.why).accessibilityIdentifier("work-today-why")
            LabeledContent("Next", value: row.next)
            LabeledContent("Time", value: row.minutes.shown)
            ForEach(Array(row.basis.enumerated()), id: \.offset) { _, basis in
                LabeledContent(basis.kind.shown, value: basis.text)
            }
            ForEach(Array(row.missing.enumerated()), id: \.offset) { _, missing in
                Label(missing, systemImage: "questionmark.circle").foregroundStyle(.secondary)
            }
        }
    }
}

/// The user's due date, in a time zone: the item's own, else this device's. Only the
/// user sets one; a plan's estimated deadline stays in its reasons (ADR 0047 §6).
struct DueSection: View {
    @Environment(AppModel.self) private var model
    let item: WorkItem
    @State private var date = DueSection.defaultDate
    @State private var saving = false
    @State private var error: String?

    /// Tomorrow at this hour, on the hour.
    private static var defaultDate: Date {
        let hour = Calendar.current.dateInterval(of: .hour, for: Date())?.start ?? Date()
        return hour.addingTimeInterval(86_400)
    }

    private var zone: TimeZone {
        item.due.flatMap { TimeZone(identifier: $0.timezone) } ?? .current
    }

    var body: some View {
        Section {
            if let due = item.due {
                LabeledContent("Due", value: due.shown).accessibilityIdentifier("work-due")
            }
            DatePicker("Date", selection: $date, displayedComponents: [.date, .hourAndMinute])
                .environment(\.timeZone, zone)
                .accessibilityIdentifier("work-due-picker")
            Button(item.due == nil ? "Set Due Date" : "Change Due Date", systemImage: Icon.due, action: save)
                .disabled(saving || !model.isConnected)
                .accessibilityIdentifier("work-set-due")
            if let error {
                Label(error, systemImage: Icon.warning).foregroundStyle(Theme.failure)
                    .accessibilityIdentifier("work-due-error")
            }
        } header: {
            Text("Due")
        } footer: {
            Text("In \(zone.identifier).")
        }
        .onAppear {
            if let due = item.due { date = Date(timeIntervalSince1970: TimeInterval(due.at)) }
        }
    }

    private func save() {
        saving = true
        error = nil
        Task {
            defer { saving = false }
            error = await model.setDue(item, date, timeZone: zone)
        }
    }
}

/// A Result's proposed `NOTES.md` for its workspace's assistant (ADR 0047 §1). The app
/// has no copy of the live file, so it shows the whole proposed text. Update Notes
/// writes it only while the host's file is still the one the run read.
struct NotesSection: View {
    @Environment(AppModel.self) private var model
    let request: AssistantRequest
    let result: AssistResult
    let notes: Notes
    @State private var applying = false
    @State private var error: String?

    var body: some View {
        Section {
            Text(notes.text.isEmpty ? "Empty the notes." : notes.text)
                .font(.callout.monospaced())
                .textSelection(.enabled)
                .accessibilityIdentifier("work-notes-text")
            if model.session.notesApplied(request, result) {
                Label("Notes updated on the host.", systemImage: Icon.done).foregroundStyle(Theme.done)
                    .accessibilityIdentifier("work-notes-applied")
            } else {
                Button("Update Notes", systemImage: Icon.note, action: apply)
                    .disabled(applying || !model.isConnected)
                    .accessibilityIdentifier("work-update-notes")
            }
            if let error {
                Label(error, systemImage: Icon.warning).foregroundStyle(Theme.failure)
                    .accessibilityIdentifier("work-notes-error")
            }
        } header: {
            Text("Proposed Notes")
        } footer: {
            Text("The full new NOTES.md of \(request.workspace)'s assistant. Nothing changes until you update it.")
        }
    }

    private func apply() {
        applying = true
        error = nil
        Task {
            defer { applying = false }
            error = await model.applyNotes(request, result)
        }
    }
}
