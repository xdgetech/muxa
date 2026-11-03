import MuxaProtocol
import MuxaSession
import SwiftUI

/// The host's Routines (ADR 0026). They are configured in `workspaces.toml` on the
/// host; the app shows their state and starts a run now.
struct RoutinesScreen: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        List {
            if let error = model.session.workError {
                Label(error, systemImage: Icon.warning).foregroundStyle(Theme.failure)
            }
            ForEach(model.session.routines, id: \.id) { routine in
                NavigationLink(value: InboxRoute.routine(workspace: routine.workspace, name: routine.name)) {
                    VStack(alignment: .leading, spacing: 2) {
                        Text(routine.name).font(.headline)
                        Text("\(routine.workspace) · \(routine.triggerTitle)").font(.caption).foregroundStyle(.secondary)
                        Text(routine.stateTitle).font(.caption).foregroundStyle(.secondary)
                    }
                }
                .accessibilityIdentifier("routine-\(routine.id)")
            }
        }
        .overlay {
            if model.session.routines.isEmpty, model.session.workError == nil {
                ContentUnavailableView("No Routines", systemImage: Icon.routine,
                                       description: Text("Add [[routine]] to workspaces.toml on the host."))
            }
        }
        .navigationTitle("Routines")
        .navigationBarTitleDisplayMode(.inline)
        .refreshable { await model.session.loadWork() }
        .task { await model.session.loadWork() }
    }
}

/// One Routine: its configuration, its runs, and Run now.
struct RoutineDetail: View {
    @Environment(AppModel.self) private var model
    let workspace: String
    let name: String
    @State private var running = false
    @State private var failure: String?

    var body: some View {
        if let routine = model.routine(workspace: workspace, name: name) {
            form(routine)
        } else {
            ContentUnavailableView("Routine Not Found", systemImage: Icon.routine,
                                   description: Text("The host no longer lists this Routine."))
        }
    }

    private func form(_ r: RoutineInfo) -> some View {
        Form {
            Section {
                field("Workspace", r.workspace, "routine-workspace")
                field("Trigger", r.triggerTitle, "routine-trigger")
                field("Enabled", r.enabled ? "Yes" : "No", "routine-enabled")
                field("Worker", r.worker ?? "Workspace default", "routine-worker")
                if let project = r.project { field("Project", project, "routine-project") }
            }
            Section("Runs") {
                if let run = r.running {
                    field("Running", "since \(run.startedAt.shownDate)", "routine-running")
                    if let s = model.runSession(r) {
                        Button("Open Terminal", systemImage: Icon.agent) { model.openTerminal(s) }
                            .accessibilityIdentifier("routine-open-terminal")
                    }
                }
                field("Next due", r.nextDue?.shownDate ?? "Not scheduled", "routine-next-due")
                field("Last success", r.lastSuccess.map { "\($0.at.shownDate) · \($0.published) new" } ?? "None yet",
                      "routine-last-success")
                if let f = r.lastFailure {
                    field("Last failure", f.at.shownDate, "routine-last-failure")
                    Text(f.reason).font(.caption).foregroundStyle(Theme.failure)
                        .accessibilityIdentifier("routine-failure-reason")
                } else {
                    field("Last failure", "None", "routine-last-failure")
                }
            }
            Section {
                Button("Run Now", systemImage: "play") { runNow(r) }
                    .disabled(!r.enabled || r.running != nil || running)
                    .accessibilityIdentifier("routine-run-now")
            } footer: {
                if !r.enabled {
                    Text("Enable it in workspaces.toml on the host. Nothing runs while it is disabled.")
                } else if r.running != nil {
                    Text("A Routine runs once at a time.")
                } else {
                    Text("Starts a run on the host now. Its briefings and proposals come to Inbox.")
                }
            }
        }
        .navigationTitle(r.name)
        .navigationBarTitleDisplayMode(.inline)
        .alert("Can't Run the Routine", isPresented: Binding(get: { failure != nil }, set: { if !$0 { failure = nil } })) {
            Button("OK", role: .cancel) {}
        } message: {
            Text(failure ?? "")
        }
    }

    private func field(_ title: String, _ value: String, _ id: String) -> some View {
        LabeledContent(title, value: value)
            .accessibilityElement(children: .combine)
            .accessibilityIdentifier(id)
    }

    private func runNow(_ r: RoutineInfo) {
        running = true
        Task {
            defer { running = false }
            do {
                _ = try await model.session.runRoutine(r)
            } catch {
                failure = PaneSession.describe(error)
            }
        }
    }
}

extension RoutineInfo {
    var id: String { "\(workspace)/\(name)" }

    var triggerTitle: String {
        trigger.map { "\($0.at) \($0.timezone)" } ?? "Run now only"
    }

    /// Running, disabled, or when it runs next.
    var stateTitle: String {
        if running != nil { return "Running" }
        if !enabled { return "Disabled" }
        if let next = nextDue { return "Next \(next.shownDate)" }
        return "Enabled"
    }
}
