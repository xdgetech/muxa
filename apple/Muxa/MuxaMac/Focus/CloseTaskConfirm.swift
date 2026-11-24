import MuxaV2
import SwiftUI

/// Asks before a Task closes from Focus (ADR 0079), then again with the
/// host's reason when its worktree holds work, as Approve does. A talk with a
/// Bot is asked about as a chat: it has a folder only, so no worktree to lose.
struct CloseTaskConfirm: ViewModifier {
    @Environment(NextModel.self) private var model
    @Environment(MacChrome.self) private var chrome
    @State private var wouldLoseWork: (task: MuxaV2.Terminal, why: String)?

    func body(content: Content) -> some View {
        content
            .alert(
                chrome.closingTask.map(title) ?? "",
                isPresented: Binding(get: { chrome.closingTask != nil }, set: { if !$0 { chrome.closingTask = nil } }),
                presenting: chrome.closingTask
            ) { task in
                Button(task.isTalk ? "Close Chat" : "Close Task", role: .destructive) { Task { await close(task, force: false) } }
                    .accessibilityIdentifier("closetask.confirm")
                Button("Cancel", role: .cancel) {}
            } message: { task in
                Text(message(task)).accessibilityIdentifier("closetask.message")
            }
            .alert(
                "Remove the worktree anyway?",
                isPresented: Binding(get: { wouldLoseWork != nil }, set: { if !$0 { wouldLoseWork = nil } }),
                presenting: wouldLoseWork
            ) { pending in
                Button("Remove Anyway", role: .destructive) { Task { await close(pending.task, force: true) } }
                    .accessibilityIdentifier("closetask.force")
                Button("Cancel", role: .cancel) {}
            } message: { pending in
                Text(pending.why)
            }
    }

    private func title(_ task: MuxaV2.Terminal) -> String {
        if let bot = task.talkBot { return "Close this chat with \(bot)?" }
        return "Close “\(task.title)”?"
    }

    /// `close-task` marks the talk's open items Done: its proposal and the items it added.
    private func message(_ task: MuxaV2.Terminal) -> String {
        guard let bot = task.talkBot else {
            return "Its agent and shells stop. Its branch worktrees are removed when they are clean and merged."
        }
        let ends = "\(bot) stops answering and the chat leaves the list. You cannot open it again."
        switch model.inbox.items.filter({ $0.terminal == task.id }).count {
        case 0: return ends
        case 1: return ends + " Its Inbox item is marked Done."
        case let n: return ends + " Its \(n) Inbox items are marked Done."
        }
    }

    private func close(_ task: MuxaV2.Terminal, force: Bool) async {
        if let why = await model.closeTask(task.id, force: force) {
            wouldLoseWork = (task, why)
        }
    }
}

extension MuxaV2.Terminal {
    /// A talk with a Bot (ADR 0083): a Task agent with `muxa.bot`. A member
    /// carries `muxa.bot` too, but runs as a handoff, not a Task.
    var isTalk: Bool { talkBot != nil }

    var talkBot: String? { isTaskAgent ? bot : nil }
}
