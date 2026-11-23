import MuxaV2
import SwiftUI

/// What an item can do with the Task or Routine behind it (ADR 0072):
/// Request changes, Retry, Open in Focus, Resume Task, Start Task and Make it a Task.
struct ItemTaskActions: View {
    @Environment(NextModel.self) private var model
    @Environment(NextNavigation.self) private var nav
    let item: InboxItem
    var onDone: () -> Void
    @State private var requesting = false
    @State private var comment = ""
    @FocusState private var commentFocused: Bool

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            // A phone is too narrow for the buttons in one row.
            ViewThatFits(in: .horizontal) {
                HStack { buttons }
                VStack(alignment: .leading) { buttons }
            }
            .buttonStyle(QuietButtonStyle())
            if requesting { commentField }
        }
    }

    @ViewBuilder
    private var buttons: some View {
        if item.closesTask {
            Button("Request Changes", systemImage: "text.bubble") {
                requesting.toggle()
                commentFocused = requesting
            }
            .keyboardShortcut("r", modifiers: .command)
            .accessibilityIdentifier("detail.requestchanges")
        }
        if item.canRetry {
            Button("Retry", systemImage: "arrow.clockwise") { Task { await act(.inboxRetry(item: item.id)) } }
                .keyboardShortcut("r", modifiers: .command)
                .accessibilityIdentifier("detail.retry")
        }
        if let t = liveTerminal {
            Button("Open in Focus", systemImage: "terminal") { nav.open(t) }
                .accessibilityIdentifier("detail.open")
        }
        if item.canResumeTask {
            Button("Resume Task", systemImage: "play") { Task { await resume() } }
                .accessibilityIdentifier("detail.resumetask")
        }
        if item.canStartTask {
            Button("Start Task", systemImage: "play") { nav.newTask = seed }
                .accessibilityIdentifier("detail.starttask")
        }
        if item.canMakeTask {
            Button("Make it a Task", systemImage: "play") { nav.newTask = seed }
                .accessibilityIdentifier("detail.maketask")
        }
    }

    /// The Start Task sheet, prefilled from the item.
    private var seed: TaskSeed {
        TaskSeed(title: item.title, goal: item.taskGoal, workspace: item.workspace)
    }

    private var liveTerminal: MuxaV2.Terminal? {
        guard let id = item.terminal else { return nil }
        return model.terminals.first { $0.id == id && $0.live }
    }

    /// One line only: several lines go as a bracketed paste, which no probe has
    /// confirmed for the agents yet (ADR 0072 decision 3).
    private var commentField: some View {
        HStack {
            TextField("What should the agent change?", text: $comment)
                .focused($commentFocused)
                .onSubmit { Task { await requestChanges() } }
                .fieldWell(radius: 10)
                .accessibilityIdentifier("detail.comment")
            Button("Send") { Task { await requestChanges() } }
                .buttonStyle(FilledButtonStyle())
                .disabled(line.isEmpty)
                .accessibilityIdentifier("detail.send")
        }
    }

    private var line: String {
        comment.split(whereSeparator: \.isNewline).joined(separator: " ").trimmingCharacters(in: .whitespaces)
    }

    private func requestChanges() async {
        guard !line.isEmpty else { return }
        await act(.inboxRequestChanges(item: item.id, comment: line))
        if model.requestError == nil {
            comment = ""
            requesting = false
        }
    }

    private func resume() async {
        guard let t = await model.resumeTask(item) else { return }
        if model.requestError == nil { onDone() }
        nav.open(t)
    }

    private func act(_ r: V2Request) async {
        await model.inbox(r)
        if model.requestError == nil { onDone() }
    }
}
