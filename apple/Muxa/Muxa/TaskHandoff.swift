import MuxaProtocol
import MuxaSession
import SwiftUI
import UIKit

/// Restarts a task's agent in its pane after a handoff (ADR 0018), once the user
/// confirms. A running agent is stopped only when the user asks for exactly that.
struct RestartAgentButton: View {
    @Environment(AppModel.self) private var model
    let item: InboxItem
    @State private var asking = false
    @State private var restarting = false
    @State private var error: String?

    static let message =
        "The current agent process will be stopped. A new agent starts in the same pane and reads TASK.md and HANDOFF.md."

    var body: some View {
        let running = item.agent?.state == .running
        Button("Restart agent", systemImage: "arrow.clockwise") { asking = true }
            .disabled(restarting)
            .accessibilityIdentifier("task-restart")
            .alert("Restart the agent?", isPresented: $asking) {
                if running {
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

    private func restart(force: Bool) {
        guard !restarting else { return }
        restarting = true
        Task {
            defer { restarting = false }
            do {
                _ = try await model.session.restartAgent(task: item.record.id, force: force)
            } catch {
                self.error = PaneSession.describe(error)
            }
        }
    }
}

/// The agents that worked on a task, newest first, as the bridge recorded them.
struct TaskRunsSection: View {
    let item: InboxItem

    var body: some View {
        let lines = item.runLines(now: UInt64(Date().timeIntervalSince1970))
        if !lines.isEmpty {
            Section("Runs") {
                ForEach(lines) { line in
                    VStack(alignment: .leading, spacing: 4) {
                        Text(line.title).font(.callout.weight(.medium))
                        Text(line.detail).font(.caption).foregroundStyle(.secondary)
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
