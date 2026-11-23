import MuxaV2
import SwiftUI

/// Asks before a Task closes from Focus (ADR 0079), then again with the
/// host's reason when its worktree holds work, as Approve does.
struct CloseTaskConfirm: ViewModifier {
    @Environment(NextModel.self) private var model
    @Environment(MacChrome.self) private var chrome
    @State private var wouldLoseWork: (task: MuxaV2.Terminal, why: String)?

    func body(content: Content) -> some View {
        content
            .alert(
                "Close “\(chrome.closingTask?.title ?? "")”?",
                isPresented: Binding(get: { chrome.closingTask != nil }, set: { if !$0 { chrome.closingTask = nil } }),
                presenting: chrome.closingTask
            ) { task in
                Button("Close Task", role: .destructive) { Task { await close(task, force: false) } }
                    .accessibilityIdentifier("closetask.confirm")
                Button("Cancel", role: .cancel) {}
            } message: { _ in
                Text("Its agent and shells stop. Its branch worktrees are removed when they are clean and merged.")
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

    private func close(_ task: MuxaV2.Terminal, force: Bool) async {
        if let why = await model.closeTask(task.id, force: force) {
            wouldLoseWork = (task, why)
        }
    }
}
