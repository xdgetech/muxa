import MuxaV2
import SwiftUI

/// A paused Task in place of its panes (ADR 0102): what it was, when it
/// paused, Resume and Close Task. Each platform asks before the close.
struct PausedTaskView: View {
    @Environment(NextModel.self) private var model
    @Environment(NextNavigation.self) private var nav
    let paused: Paused
    let close: () -> Void
    @State private var resuming = false

    var body: some View {
        let t = paused.terminal
        VStack(spacing: 16) {
            Image(systemName: Icon.pause)
                .font(.system(size: 30))
                .foregroundStyle(TaskState.paused.color)
            VStack(spacing: 4) {
                Text(t.pathText).font(.system(size: 13)).foregroundStyle(Palette.textSecondary)
                Text(t.title)
                    .font(.system(size: 19, weight: .semibold))
                    .foregroundStyle(Palette.text)
                    .multilineTextAlignment(.center)
                    .accessibilityIdentifier("paused.title")
                Text("Paused \(Self.since(paused.pausedAt))")
                    .font(.system(size: 13))
                    .foregroundStyle(Palette.textTertiary)
                    .accessibilityIdentifier("paused.since")
            }
            Text("Its agent and shells stopped. Its places and Inbox items stay. Resume runs the agent again where it worked.")
                .font(.system(size: 13))
                .foregroundStyle(Palette.textSecondary)
                .multilineTextAlignment(.center)
            HStack(spacing: 10) {
                Button("Close Task…", action: close)
                    .buttonStyle(QuietButtonStyle())
                    .accessibilityIdentifier("paused.close")
                Button("Resume", systemImage: Icon.resume) {
                    resuming = true
                    Task {
                        await nav.resume(paused, model: model)
                        resuming = false
                    }
                }
                .buttonStyle(FilledButtonStyle())
                .disabled(resuming)
                .accessibilityIdentifier("paused.resume")
            }
        }
        .padding(24)
        .frame(maxWidth: 440)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(Palette.ground)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("paused.view")
    }

    /// `2 hours ago`, from Unix seconds.
    static func since(_ seconds: Int64) -> String {
        Date(timeIntervalSince1970: TimeInterval(seconds)).formatted(.relative(presentation: .named))
    }
}

/// Pause Task for a live Task's agent, Resume Task for a paused one: a Task
/// row's menu items.
struct PauseMenuItems: View {
    @Environment(NextModel.self) private var model
    @Environment(NextNavigation.self) private var nav
    let terminal: MuxaV2.Terminal

    var body: some View {
        if let p = model.pausedRecord(terminal.id) {
            Button("Resume Task", systemImage: Icon.resume) { Task { await nav.resume(p, model: model) } }
        } else if terminal.isTaskAgent, terminal.live {
            Button("Pause Task", systemImage: Icon.pause) { Task { await nav.pause(terminal.id, model: model) } }
        }
    }
}

extension View {
    /// Pause Task or Resume Task on a Task's row; other terminals get no menu.
    @ViewBuilder
    func pauseMenu(_ terminal: MuxaV2.Terminal) -> some View {
        if terminal.isTaskAgent {
            contextMenu { PauseMenuItems(terminal: terminal) }
        } else {
            self
        }
    }
}

extension NextNavigation {
    /// Pauses the Task. When it is the open one, it stays open on its Paused
    /// view, without its shells' panes.
    func pause(_ terminal: String, model: NextModel) async {
        let open = self.terminal == terminal
        guard await model.pauseTask(terminal), open else { return }
        closeTask()
        self.terminal = terminal
    }

    /// Resumes the paused Task and opens its new agent, as opening a Task does.
    func resume(_ paused: Paused, model: NextModel) async {
        if let t = await model.resumePaused(paused) { open(t) }
    }
}
