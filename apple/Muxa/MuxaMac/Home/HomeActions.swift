import MuxaV2
import SwiftUI

/// The detail's action bar on the Mac: the requests of `InboxItemPane` and
/// `ItemTaskActions` (ADR 0072), drawn as the mock's buttons with their keys.
struct HomeActions: View {
    @Environment(NextModel.self) private var model
    @Environment(NextNavigation.self) private var nav
    let item: InboxItem
    /// False for a Done item: only Reopen.
    let live: Bool
    let terminal: MuxaV2.Terminal?
    var onDone: () -> Void
    @State private var requesting = false
    @State private var comment = ""
    @FocusState private var commentFocused: Bool
    /// The server's reason not to remove a worktree, until the user decides.
    @State private var wouldLoseWork: String?

    var body: some View {
        VStack(spacing: 0) {
            if requesting { commentField }
            ActionBar {
                if live { buttons } else { reopen }
            }
        }
        .alert("Remove the worktree anyway?", isPresented: losingWork, presenting: wouldLoseWork) { _ in
            Button("Remove Anyway", role: .destructive) { Task { await approve(force: true) } }
                .accessibilityIdentifier("detail.force")
            Button("Cancel", role: .cancel) {}
        } message: { Text($0) }
    }

    @ViewBuilder
    private var buttons: some View {
        Button {
            Task {
                if item.closesTask { await approve(force: false) } else { await act(.inboxDone(item: item.id)) }
            }
        } label: {
            ActionLabel(title: item.closesTask ? "Approve" : "Done", key: Shortcut.open.display, primary: true)
        }
        .buttonStyle(ActionButtonStyle(kind: .primary))
        .keyboardShortcut(.return, modifiers: .command)
        .accessibilityIdentifier("detail.main")
        if item.closesTask {
            Button { requesting.toggle(); commentFocused = requesting } label: {
                ActionLabel(title: "Request changes", key: Shortcut.retry.display)
            }
            .buttonStyle(ActionButtonStyle())
            .keyboardShortcut("r", modifiers: .command)
            .accessibilityIdentifier("detail.requestchanges")
        }
        if item.canRetry {
            Button { Task { await act(.inboxRetry(item: item.id)) } } label: {
                ActionLabel(title: "Retry", key: Shortcut.retry.display)
            }
            .buttonStyle(ActionButtonStyle())
            .keyboardShortcut("r", modifiers: .command)
            .accessibilityIdentifier("detail.retry")
        }
        if let terminal {
            Button { nav.open(terminal) } label: { ActionLabel(title: terminal.belongsToBot ? "Open in Bots" : "Open in Focus", key: "⏎") }
                .buttonStyle(ActionButtonStyle())
                .accessibilityIdentifier("detail.open")
        }
        if item.canResumeTask {
            Button { Task { await resume() } } label: { ActionLabel(title: "Resume Task") }
                .buttonStyle(ActionButtonStyle())
                .accessibilityIdentifier("detail.resumetask")
        }
        if item.canStartTask {
            Button { nav.newTask = seed } label: { ActionLabel(title: "Start Task") }
                .buttonStyle(ActionButtonStyle())
                .accessibilityIdentifier("detail.starttask")
        }
        if item.canMakeTask {
            Button { nav.newTask = seed } label: { ActionLabel(title: "Make it a Task") }
                .buttonStyle(ActionButtonStyle())
                .accessibilityIdentifier("detail.maketask")
        }
        laterMenu
        if item.closesTask {
            Button { Task { await act(.inboxDone(item: item.id)) } } label: {
                ActionLabel(title: "Done", key: Shortcut.done.display)
            }
            .buttonStyle(ActionButtonStyle())
            .accessibilityIdentifier("detail.done")
        }
        Spacer(minLength: 0)
        moreMenu
        hiddenKeys
    }

    private var reopen: some View {
        Button { Task { await model.inbox(.inboxReopen(item: item.id)) } } label: { ActionLabel(title: "Reopen") }
            .buttonStyle(ActionButtonStyle())
            .accessibilityIdentifier("detail.reopen")
    }

    private var laterMenu: some View {
        Menu {
            ForEach(LaterChoice.allCases) { choice in
                Button(choice.title) { Task { await act(.inboxLater(item: item.id, until: choice.until())) } }
            }
        } label: {
            ActionLabel(title: "Later", key: Shortcut.later.display)
        }
        .menuStyle(.button)
        .menuIndicator(.hidden)
        .buttonStyle(ActionButtonStyle())
        .fixedSize()
        .accessibilityIdentifier("detail.later")
    }

    private var moreMenu: some View {
        Menu {
            Button("Delete", role: .destructive) { Task { await act(.inboxDelete(item: item.id)) } }
        } label: {
            Image(systemName: "ellipsis").accessibilityLabel("More")
        }
        .menuStyle(.button)
        .menuIndicator(.hidden)
        .buttonStyle(ActionButtonStyle())
        .fixedSize()
    }

    /// `⌘L`: Later until tomorrow morning, the usual choice. `⌘⌫`: Done.
    private var hiddenKeys: some View {
        Group {
            Button("") { Task { await act(.inboxLater(item: item.id, until: LaterChoice.tomorrow.until())) } }
                .keyboardShortcut("l", modifiers: .command)
            Button("") { Task { await act(.inboxDone(item: item.id)) } }
                .keyboardShortcut(.delete, modifiers: .command)
        }
        .frame(width: 0, height: 0)
        .opacity(0)
        .accessibilityHidden(true)
    }

    /// One line only: several lines go as a bracketed paste, which no probe has
    /// confirmed for the agents yet (ADR 0072 decision 3).
    private var commentField: some View {
        HStack(spacing: 8) {
            TextField("What should the agent change?", text: $comment)
                .textFieldStyle(.plain)
                .focused($commentFocused)
                .onSubmit { Task { await requestChanges() } }
                .padding(.horizontal, 12)
                .frame(height: 36)
                .background(Palette.surface, in: RoundedRectangle(cornerRadius: 8))
                .overlay(RoundedRectangle(cornerRadius: 8).strokeBorder(Palette.borderStrong))
                .accessibilityIdentifier("detail.comment")
            Button { Task { await requestChanges() } } label: { ActionLabel(title: "Send", key: "⏎", primary: true) }
                .buttonStyle(ActionButtonStyle(kind: .primary))
                .disabled(line.isEmpty)
                .accessibilityIdentifier("detail.send")
        }
        .padding(.horizontal, 28)
        .padding(.vertical, 12)
        .overlay(alignment: .top) { Rectangle().fill(Palette.border).frame(height: 1) }
    }

    private var line: String {
        comment.split(whereSeparator: \.isNewline).joined(separator: " ").trimmingCharacters(in: .whitespaces)
    }

    private var seed: TaskSeed {
        TaskSeed(title: item.title, goal: item.note, workspace: item.workspace)
    }

    private var losingWork: Binding<Bool> {
        Binding(get: { wouldLoseWork != nil }, set: { if !$0 { wouldLoseWork = nil } })
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

    private func approve(force: Bool) async {
        if let message = await model.approve(item.id, force: force) {
            wouldLoseWork = message
        } else if model.requestError == nil {
            onDone()
        }
    }
}
