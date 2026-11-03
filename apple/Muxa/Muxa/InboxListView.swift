import MuxaProtocol
import MuxaSession
import SwiftUI

/// The Inbox as one ordered list (ADR 0025 "List first"): Task needs in the host's
/// order, waiting Routine runs, briefings and proposals. A row opens its detail
/// (Act); its menu has Done, Move down and Later where they apply.
struct InboxListView: View {
    @Environment(AppModel.self) private var model
    @Binding var focused: String?
    @Binding var path: [InboxRoute]
    @State private var prompt: InboxPrompt?
    @State private var failure: String?

    var body: some View {
        let rows = model.inboxRows
        List {
            ForEach(rows) { row in
                rowView(row)
                    .listRowBackground(row.id == focused ? Color.accentColor.opacity(0.12) : nil)
                    .swipeActions(edge: .trailing) { swipe(row) }
            }
            if model.inboxSnoozedCount > 0 {
                Section {
                    Button("Show snoozed (\(model.inboxSnoozedCount))") { model.inboxSnoozes = [:] }
                        .accessibilityIdentifier("inbox-show-snoozed")
                }
            }
        }
        .accessibilityIdentifier("inbox-list")
        .overlay {
            if rows.isEmpty {
                ContentUnavailableView("Nothing Needs You", systemImage: "checkmark.circle",
                                       description: Text("All Tasks keeps every Task, running and closed."))
                    .accessibilityIdentifier("inbox-empty")
            }
        }
        .onChange(of: rows.map(\.id)) { _, ids in
            if let focused, !ids.contains(focused) { self.focused = ids.first }
        }
        .inboxActions($prompt) { task in path = [.task(task)] }
        .alert("Can't Update the Inbox", isPresented: Binding(get: { failure != nil }, set: { if !$0 { failure = nil } })) {
            Button("OK", role: .cancel) {}
        } message: {
            Text(failure ?? "")
        }
        .background { shortcuts(rows) }
    }

    private func rowView(_ row: InboxRow) -> some View {
        HStack {
            Button { act(row) } label: {
                Label {
                    VStack(alignment: .leading, spacing: 2) {
                        Text(row.title).font(.headline)
                        Text(row.subtitle).font(.caption).foregroundStyle(.secondary)
                    }
                } icon: {
                    Image(systemName: row.icon).foregroundStyle(.secondary)
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                .contentShape(Rectangle())
            }
            .buttonStyle(.borderless)
            .foregroundStyle(.primary)
            .accessibilityIdentifier("inbox-row-\(row.id)")
            Menu {
                menu(row)
            } label: {
                Image(systemName: "ellipsis.circle").imageScale(.large)
            }
            .buttonStyle(.borderless)
            .accessibilityLabel("Actions")
            .accessibilityIdentifier("inbox-actions-\(row.id)")
        }
    }

    /// Act, Done, Move down and Later, as the row and the host allow.
    @ViewBuilder private func menu(_ row: InboxRow) -> some View {
        switch row {
        case .task(let item):
            Button("Open", systemImage: Icon.tasks) { act(row) }
            if model.canPlaceTasks {
                Button("Move Down", systemImage: Icon.moveDown) { perform { try await model.moveDown(row) } }
                    .accessibilityIdentifier("inbox-move-down")
                Button("Later…", systemImage: Icon.later) { prompt = .later(row) }
                    .accessibilityIdentifier("inbox-later")
            } else {
                Button("Snooze", systemImage: Icon.later) { model.snooze(item) }
                    .accessibilityIdentifier("inbox-snooze")
            }
            Button("Mark Done", systemImage: "checkmark") { perform { try await model.done(row) } }
                .accessibilityIdentifier("inbox-done")
        case .entry(let entry):
            Button("Open", systemImage: row.icon) { act(row) }
            if entry.hasKnownActions {
                if entry.isProposal {
                    Button("Approve…", systemImage: "checkmark.seal") { prompt = .approve(entry) }
                        .accessibilityIdentifier("inbox-approve")
                    Button("Dismiss", systemImage: "xmark") { perform { try await model.dismiss(entry) } }
                        .accessibilityIdentifier("inbox-dismiss")
                } else {
                    Button("Done", systemImage: "checkmark") { perform { try await model.done(row) } }
                        .accessibilityIdentifier("inbox-done")
                }
                Button("Later…", systemImage: Icon.later) { prompt = .later(row) }
                    .accessibilityIdentifier("inbox-later")
            }
        case .routine(let need):
            Button("Open Terminal", systemImage: Icon.agent) { act(row) }
            Button("Routine", systemImage: Icon.routine) {
                path = [.routine(workspace: need.routine.workspace, name: need.routine.name)]
            }
        }
    }

    @ViewBuilder private func swipe(_ row: InboxRow) -> some View {
        switch row {
        case .task where model.canPlaceTasks:
            Button("Later", systemImage: Icon.later) { prompt = .later(row) }.tint(.indigo)
            Button("Move Down", systemImage: Icon.moveDown) { perform { try await model.moveDown(row) } }.tint(.gray)
        case .entry(let entry) where entry.hasKnownActions:
            Button("Later", systemImage: Icon.later) { prompt = .later(row) }.tint(.indigo)
        default:
            EmptyView()
        }
    }

    /// Opens the row's detail; a waiting Routine run opens its terminal.
    private func act(_ row: InboxRow) {
        focused = row.id
        switch row {
        case .task(let item): path.append(.task(item.id))
        case .entry(let entry): path.append(.entry(entry.id))
        case .routine(let need): model.openTerminal(need.session)
        }
    }

    private func perform(_ action: @escaping () async throws -> Void) {
        Task {
            do { try await action() } catch { failure = PaneSession.describe(error) }
        }
    }

    /// Cmd+Up/Down move the focus, Cmd+O opens, Cmd+Shift+Down moves down,
    /// Cmd+Shift+L asks for Later, and Cmd+S snoozes on an older host.
    @ViewBuilder private func shortcuts(_ rows: [InboxRow]) -> some View {
        let current = rows.first { $0.id == focused } ?? rows.first
        Group {
            Button("Previous") { step(rows, -1) }.keyboardShortcut(.upArrow)
            Button("Next") { step(rows, 1) }.keyboardShortcut(.downArrow)
            Button("Open") { if let current { act(current) } }.keyboardShortcut("o")
            if let current, case .task(let item) = current {
                if model.canPlaceTasks {
                    Button("Move Down") { perform { try await model.moveDown(current) } }
                        .keyboardShortcut(.downArrow, modifiers: [.command, .shift])
                    Button("Later") { prompt = .later(current) }.keyboardShortcut("l", modifiers: [.command, .shift])
                } else {
                    Button("Snooze") { model.snooze(item) }.keyboardShortcut("s")
                }
            } else if let current, case .entry(let entry) = current, entry.hasKnownActions {
                Button("Later") { prompt = .later(current) }.keyboardShortcut("l", modifiers: [.command, .shift])
            }
        }
        .hidden()
    }

    private func step(_ rows: [InboxRow], _ offset: Int) {
        guard !rows.isEmpty else { return }
        let index = rows.firstIndex { $0.id == focused } ?? 0
        focused = rows[min(max(index + offset, 0), rows.count - 1)].id
    }
}
