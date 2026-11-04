import MuxaProtocol
import MuxaSession
import SwiftUI
import UIKit

/// Restarts a task's agent in its pane after a handoff (ADR 0018), once the user
/// confirms. A running agent is stopped only when the user asks for exactly that.
struct RestartAgentButton: View {
    @Environment(AppModel.self) private var model
    let item: MuxaSession.InboxItem
    @State private var asking = false
    @State private var restarting = false
    @State private var choosingWorker = false
    /// A worker name or `default`; a worker-aware restart always sends one.
    @State private var workerChoice = defaultWorker
    @State private var error: String?

    static let message =
        "The current agent process will be stopped. A new agent starts in the same pane and reads TASK.md and HANDOFF.md."

    var body: some View {
        let needsForce = item.restartNeedsForce
        Button("Restart agent", systemImage: "arrow.clockwise") {
            workerChoice = item.record.worker ?? defaultWorker
            if model.session.canUseWorkers { choosingWorker = true } else { asking = true }
        }
            .disabled(restarting || item.record.restartOperation != nil)
            .accessibilityIdentifier("task-restart")
            .sheet(isPresented: $choosingWorker) {
                NavigationStack {
                    Form {
                        Text(Self.message)
                        Picker("Worker", selection: $workerChoice) {
                            Text("Workspace default").tag(defaultWorker)
                            ForEach(workerNames, id: \.self) { name in
                                Text(name == item.record.worker ? "\(name) (current)" : name).tag(name)
                            }
                        }
                        .accessibilityIdentifier("restart-worker")
                        Button(needsForce ? "Stop and restart" : "Restart", role: needsForce ? .destructive : nil) {
                            choosingWorker = false
                            restart(force: needsForce)
                        }
                        .disabled(workerIssue != nil)
                        .accessibilityIdentifier(needsForce ? "restart-force" : "restart-confirm")
                        if let workerIssue { Text(workerIssue).foregroundStyle(Theme.failure) }
                    }
                    .navigationTitle("Restart the agent?")
                    .toolbar { ToolbarItem(placement: .cancellationAction) { Button("Cancel") { choosingWorker = false } } }
                }
            }
            .alert("Restart the agent?", isPresented: $asking) {
                if needsForce {
                    Button("Stop and restart", role: .destructive) { restart(force: true) }
                        .accessibilityIdentifier("restart-force")
                } else {
                    Button("Restart") { restart(force: false) }
                        .accessibilityIdentifier("restart-confirm")
                }
                Button("Cancel", role: .cancel) {}
            } message: {
                Text(Self.message)
            }
            .alert("Can't Restart the Agent", isPresented: Binding(get: { error != nil }, set: { if !$0 { error = nil } })) {
                Button("OK", role: .cancel) {}
            } message: {
                Text(error ?? "")
            }
    }

    /// Allowed workers, plus the current one when the host no longer allows it,
    /// so the picker shows it with its problem instead of switching silently.
    private var workerNames: [String] {
        var names = model.session.workers.filter { $0.allows(workspace: item.record.workspace) }.map(\.name)
        if let current = item.record.worker, !names.contains(current) { names.insert(current, at: 0) }
        return names
    }

    /// Without the capability the request stays legacy: no worker, so Keep.
    private var selectedWorker: String? { model.session.canUseWorkers ? workerChoice : nil }

    private var workerIssue: String? {
        workerProblem(name: selectedWorker, workspace: item.record.workspace, workers: model.session.workers)
    }

    private func restart(force: Bool) {
        guard !restarting, workerIssue == nil else { return }
        restarting = true
        Task {
            defer { restarting = false }
            do {
                _ = try await model.session.restartAgent(task: item.record.id, force: force,
                    worker: selectedWorker, rev: item.record.rev)
            } catch {
                self.error = PaneSession.describe(error)
            }
        }
    }
}

/// The agents that worked on a task, newest first, as the bridge recorded them.
struct TaskRunsSection: View {
    let item: MuxaSession.InboxItem

    var body: some View {
        let lines = item.runLines(now: UInt64(Date().timeIntervalSince1970))
        if !lines.isEmpty {
            Section("Runs") {
                ForEach(lines) { line in
                    VStack(alignment: .leading, spacing: 4) {
                        Text(line.title).font(.callout.weight(.medium))
                        Text(line.detail).font(.caption).foregroundStyle(.secondary)
                        if let assignment = line.assignment { Text(assignment).font(.caption).foregroundStyle(.secondary) }
                        if let session = line.agentSession {
                            HStack {
                                Text(session).font(.caption.monospaced()).textSelection(.enabled)
                                Spacer()
                                Button("Copy session id", systemImage: "doc.on.doc") {
                                    UIPasteboard.general.string = session
                                }
                                .labelStyle(.iconOnly)
                                .buttonStyle(.borderless)
                                .accessibilityIdentifier("run-copy-\(line.id)")
                            }
                        }
                    }
                    .accessibilityElement(children: .contain)
                    .accessibilityIdentifier("run-\(line.id)")
                }
            }
        }
    }
}
