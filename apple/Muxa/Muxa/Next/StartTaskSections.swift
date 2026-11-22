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

/// Where the agent works: a new folder, branch worktrees, read-only repos.
struct TaskPlacesSection: View {
    @Binding var draft: TaskDraft
    let workspace: Workspace

    var body: some View {
        Section {
            Toggle("New folder", isOn: $draft.folder)
                .accessibilityIdentifier("task.folder")
            ForEach(workspace.repos, id: \.self) { repo in
                Picker(repo, selection: use(repo)) {
                    ForEach(TaskDraft.RepoUse.allCases) { Text($0.rawValue).tag($0) }
                }
                .accessibilityIdentifier("task.repo.\(repo)")
                if draft.use(of: repo) == .branch {
                    TextField("Branch in \(repo)", text: branch(repo))
                        .plainEntry()
                        .accessibilityIdentifier("task.branch.\(repo)")
                }
            }
        } header: {
            Text("Places")
        } footer: {
            Text("The agent works in the folder, else in the first branch. Read only is a rule for the agent.")
        }
    }

    private func use(_ repo: String) -> Binding<TaskDraft.RepoUse> {
        Binding(get: { draft.use(of: repo) }, set: { draft.setUse($0, of: repo) })
    }

    private func branch(_ repo: String) -> Binding<String> {
        Binding(get: { draft.branches[repo] ?? "" }, set: { draft.branches[repo] = $0 })
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
