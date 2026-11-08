import MuxaProtocol
import MuxaSession
import SwiftUI

/// Tasks mode (ADR 0035): Routines as the first row, then every Task grouped by what
/// it needs. A row shows its Task in the detail; a phone pushes it.
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
        ForEach(model.inbox) { section in
            Section {
                ForEach(section.items) { item in
                    Button { model.openTaskRow(item) } label: { TaskRow(item: item) }
                        .foregroundStyle(.primary)
                        .sidebarHighlight(model.highlights(item))
                        .accessibilityIdentifier("task-\(item.id)")
                }
            } header: {
                Text(section.group.title).accessibilityIdentifier("inbox-\(section.group.title)")
            } footer: {
                if section.group == model.inbox.last?.group, model.session.tasksTruncated {
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
