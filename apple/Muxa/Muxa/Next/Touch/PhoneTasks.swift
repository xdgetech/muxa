import MuxaV2
import SwiftUI

extension NextModel {
    /// The live Tasks' agents.
    var liveTasks: [Terminal] { terminals.filter { $0.live && $0.isTaskAgent } }

    /// Live terminals that are not a Task's agent: shells and the like.
    var otherTerminals: [Terminal] { terminals.filter { $0.live && !$0.isTaskAgent } }

    /// The live Tasks by Workspace, in name order; Tasks without one last.
    var tasksByWorkspace: [(name: String, tasks: [Terminal])] {
        let groups = Dictionary(grouping: liveTasks) { $0.workspace ?? "" }
        return groups.keys.sorted { a, b in a.isEmpty ? false : b.isEmpty ? true : a < b }
            .map { (name: $0.isEmpty ? "Other Tasks" : $0, tasks: groups[$0] ?? []) }
    }

    /// The live Tasks by what their agents report, in the mocks' order.
    var tasksByState: [(state: TaskState, tasks: [Terminal])] {
        TaskState.allCases.compactMap { state in
            let tasks = liveTasks.filter { $0.state == state }
            return tasks.isEmpty ? nil : (state: state, tasks: tasks)
        }
    }
}

/// The phone's Workspaces tab (PhoneSpaces mock): the Tasks by Workspace,
/// each with its state, and New Task.
struct PhoneTasks: View {
    @Environment(NextModel.self) private var model
    @Environment(NextNavigation.self) private var nav

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(spacing: 14) {
                    ScreenTitle(title: "Workspaces") {
                        NewTaskButton(id: "home.newtask")
                            .labelStyle(.iconOnly)
                            .buttonStyle(RaisedIconStyle(round: true))
                    }
                    .padding(.horizontal, 6)
                    .padding(.bottom, 4)
                    ForEach(model.tasksByWorkspace, id: \.name) { group in
                        WorkspaceCard(name: group.name, tasks: group.tasks, workspace: group.tasks.first?.workspace)
                    }
                    if !model.otherTerminals.isEmpty {
                        WorkspaceCard(name: "Terminals", tasks: model.otherTerminals, workspace: nil, canStart: false)
                    }
                    if model.liveTasks.isEmpty {
                        Text("No Tasks run. Start one with New Task.")
                            .foregroundStyle(Palette.textSecondary)
                            .padding(.top, 20)
                    }
                }
                .padding(.horizontal, 12)
                .padding(.top, 8)
                .padding(.bottom, 16)
            }
            .background(Palette.ground)
            .toolbar(.hidden, for: .navigationBar)
        }
        .tint(Palette.accent)
    }
}

/// One Workspace's Tasks in a card, as the PhoneSpaces mock.
private struct WorkspaceCard: View {
    @Environment(NextNavigation.self) private var nav
    let name: String
    let tasks: [Terminal]
    let workspace: String?
    var canStart = true

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                Text(name).font(.system(size: 16, weight: .semibold)).foregroundStyle(Palette.text)
                Spacer()
                Text(count)
                    .font(.system(size: 12))
                    .foregroundStyle(Palette.textTertiary)
            }
            .padding(.horizontal, 14)
            .padding(.top, 12)
            .padding(.bottom, 8)
            ForEach(tasks) { t in
                Divider().overlay(Palette.divider)
                TaskRow(terminal: t) { nav.open(t) }
            }
            if canStart {
                Divider().overlay(Palette.divider)
                Button { nav.newTask = TaskSeed(workspace: workspace) } label: {
                    Label("New Task", systemImage: "plus")
                        .font(.system(size: 14))
                        .frame(maxWidth: .infinity, minHeight: minTouch, alignment: .leading)
                        .padding(.horizontal, 14)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .foregroundStyle(Palette.accent)
            }
        }
        .card(border: true)
    }

    /// `2 Tasks`, or `1 terminal` in the card of other terminals.
    private var count: String {
        let noun = canStart ? "Task" : "terminal"
        return "\(tasks.count) \(noun)\(tasks.count == 1 ? "" : "s")"
    }
}

/// A Task in a Workspace's card: its state dot, title, place and state.
private struct TaskRow: View {
    let terminal: Terminal
    var open: () -> Void

    var body: some View {
        let state = terminal.state
        Button(action: open) {
            HStack(spacing: 10) {
                StateDot(color: state.color)
                VStack(alignment: .leading, spacing: 1) {
                    Text(terminal.title).foregroundStyle(Palette.text).lineLimit(1)
                    if let place = terminal.placeText {
                        Text(place).font(.system(size: 12)).foregroundStyle(Palette.textTertiary)
                    }
                }
                .padding(.vertical, 6)
                Spacer()
                if terminal.status != nil {
                    Text(state.title.lowercased()).font(.system(size: 12.5)).foregroundStyle(state.color)
                }
                Image(systemName: "chevron.right")
                    .font(.system(size: 12, weight: .bold))
                    .foregroundStyle(Palette.textTertiary)
            }
            .padding(.horizontal, 14)
            .frame(minHeight: minTouch)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier("terminal.\(terminal.id)")
    }
}
