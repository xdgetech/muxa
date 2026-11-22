import MuxaV2
import SwiftUI

/// Plan mode, and the Task Loop when the Workspace allows one.
struct TaskAgentSection: View {
    @Binding var draft: TaskDraft
    let workspace: Workspace

    var body: some View {
        Section {
            Toggle("Plan mode", isOn: $draft.plan)
                .accessibilityIdentifier("task.plan")
            if let checks = workspace.loop?.checks {
                Toggle("Task Loop", isOn: $draft.loop)
                    .accessibilityIdentifier("task.loop")
                if draft.loop { loop(checks) }
            }
        } header: {
            Text("Agent")
        } footer: {
            if workspace.loop != nil, draft.loop {
                Text("After each finished turn the host runs the check in the Task's folder.")
            }
        }
    }

    @ViewBuilder
    private func loop(_ checks: LoopChecks) -> some View {
        switch checks {
        case .any:
            TextField("Check, e.g. make test", text: $draft.check)
                .monospaced()
                .plainEntry()
                .accessibilityIdentifier("task.check")
        case .list(let list):
            Picker("Check", selection: $draft.check) {
                ForEach(list, id: \.self) { Text($0).monospaced().tag($0) }
            }
            .accessibilityIdentifier("task.check")
        }
        Stepper("Stop after \(draft.attempts) failed checks", value: $draft.attempts, in: 1...10)
            .accessibilityIdentifier("task.attempts")
    }
}

/// Where the agent works: one repo, as a branch or read only, or no repo,
/// where it reads every repo (ADR 0077).
struct TaskPlacesSection: View {
    @Binding var draft: TaskDraft
    let workspace: Workspace

    var body: some View {
        Section {
            Picker("Repo", selection: $draft.repo) {
                Text("No repo").tag(String?.none)
                ForEach(workspace.repos, id: \.self) { Text($0).tag(Optional($0)) }
            }
            .accessibilityIdentifier("task.repo")
            if let repo = draft.repo {
                Picker("Use", selection: $draft.mode) {
                    ForEach(TaskDraft.RepoMode.allCases) { Text($0.rawValue).tag($0) }
                }
                .pickerStyle(.segmented)
                .accessibilityIdentifier("task.mode")
                if draft.mode == .branch {
                    TextField("Branch in \(repo)", text: $draft.branch)
                        .plainEntry()
                        .accessibilityIdentifier("task.branch")
                }
            }
        } header: {
            Text("Repo")
        } footer: {
            Text(footer)
        }
    }

    private var footer: String {
        switch (draft.repo, draft.mode) {
        case (nil, _): "The agent works in a new folder and can read every repo of \(workspace.name)."
        case (_, .branch): "The agent works in a new worktree on this branch."
        case (_, .read): "The agent works in a new folder and reads this repo. Read only is a rule for the agent."
        }
    }
}

/// What every agent of the Workspace gets (ADR 0075); Settings changes it.
struct TaskToolsSection: View {
    let workspace: Workspace

    var body: some View {
        if let line = Self.line(for: workspace) {
            Section {
                Text(line)
                    .font(.callout)
                    .foregroundStyle(Palette.textSecondary)
                    .accessibilityIdentifier("task.tools")
            } footer: {
                Text("Change them in Settings.")
            }
        }
    }

    /// "This Workspace's agents get: github, linear · MCP: docs", or nil when it has neither.
    static func line(for ws: Workspace) -> String? {
        var parts: [String] = []
        if !ws.connections.isEmpty { parts.append(ws.connections.map(\.name).joined(separator: ", ")) }
        if !ws.mcp.isEmpty { parts.append("MCP: " + ws.mcp.map(\.name).joined(separator: ", ")) }
        return parts.isEmpty ? nil : "This Workspace's agents get: " + parts.joined(separator: " · ")
    }
}
