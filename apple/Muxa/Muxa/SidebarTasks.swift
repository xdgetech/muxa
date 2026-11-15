import MuxaProtocol
import MuxaSession
import SwiftUI

/// Tasks mode (ADR 0035): Routines as the first row, then every Task by workspace,
/// then repo (ADR 0063). Groups fold. A row shows its Task in the detail; a phone pushes it.
struct SidebarTasks: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        if let error = model.session.taskError {
            Label(error, systemImage: Icon.warning).foregroundStyle(Theme.failure)
        }
        if model.canUseRoutines {
            Section {
                Button { model.select(.routines) } label: {
                    LabeledContent {
                        Text("\(model.session.routines.count)")
                    } label: {
                        Label("Routines", systemImage: Icon.routine)
                    }
                }
                .foregroundStyle(.primary)
                .sidebarHighlight(model.detail == .routines)
                .accessibilityIdentifier("open-routines")
            }
        }
        let groups = model.taskGroups
        ForEach(groups) { group in
            Section {
                if !model.isFolded(group.foldKey) {
                    ForEach(group.repos) { repo in
                        if repo.heading { RepoFoldRow(repo: repo) }
                        if !repo.heading || !model.isFolded(repo.foldKey) {
                            ForEach(repo.items) { item in
                                Button { model.openTaskRow(item) } label: { TaskRow(item: item) }
                                    .foregroundStyle(.primary)
                                    .sidebarHighlight(model.highlights(item))
                                    .accessibilityIdentifier("task-\(item.id)")
                            }
                        }
                    }
                }
            } header: {
                FoldButton(title: group.workspace, folded: model.isFolded(group.foldKey)) {
                    model.toggleFold(group.foldKey)
                }
                .accessibilityIdentifier("task-group-\(group.workspace)")
            } footer: {
                if group.id == groups.last?.id, model.session.tasksTruncated {
                    Text("The host has more tasks than it listed.")
                }
            }
        }
        if model.inbox.isEmpty, model.session.taskError == nil {
            Text("No Tasks. Tasks you start on this host show here, with their agent's state.")
                .font(.caption).foregroundStyle(.secondary)
                .accessibilityIdentifier("tasks-empty")
        }
    }
}

/// A repo's row inside a workspace's section; a tap folds its Tasks.
private struct RepoFoldRow: View {
    @Environment(AppModel.self) private var model
    let repo: RepoTasks

    var body: some View {
        let folded = model.isFolded(repo.foldKey)
        Button { model.toggleFold(repo.foldKey) } label: {
            HStack {
                Label(repo.label, systemImage: repo.repo == nil ? Icon.tasks : Icon.repo)
                    .font(.subheadline.weight(.semibold))
                Spacer()
                if folded {
                    Text("\(repo.items.count)").font(.caption.monospacedDigit()).foregroundStyle(.secondary)
                }
                Image(systemName: "chevron.right").rotationEffect(.degrees(folded ? 0 : 90))
                    .font(.caption).foregroundStyle(.secondary)
            }
        }
        .foregroundStyle(.secondary)
        .accessibilityIdentifier("task-repo-\(repo.workspace)-\(repo.label)")
        .accessibilityValue(folded ? "folded" : "open")
    }
}

/// A workspace's section header; a tap folds the section.
private struct FoldButton: View {
    let title: String
    let folded: Bool
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            HStack {
                Text(title)
                Spacer()
                Image(systemName: "chevron.right").rotationEffect(.degrees(folded ? 0 : 90))
            }
        }
        .accessibilityValue(folded ? "folded" : "open")
    }
}

/// The detail of Tasks mode with nothing selected.
struct TasksEmptyDetail: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        if model.inbox.isEmpty {
            ContentUnavailableView("No Tasks", systemImage: Icon.tasks,
                                   description: Text("Tasks you start on this host show here, with their agent's state."))
        } else {
            ContentUnavailableView("Pick a Task", systemImage: Icon.tasks,
                                   description: Text("Its goal, its agent and the ways into its session show here."))
        }
    }
}
