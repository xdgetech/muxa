import MuxaV2
import SwiftUI

/// Which terminals the Task lists show, and where. Ended terminals are left
/// out: a failed agent already has a Failed item in the Inbox. Paused Tasks
/// are listed (ADR 0104).
extension NextModel {
    /// The live Tasks' agents.
    var liveTasks: [Terminal] { terminals.filter { $0.live && $0.isTaskAgent } }

    /// The paused Tasks' agents as they were (ADR 0104); the lists show them
    /// after the live ones.
    var pausedTasks: [Terminal] { paused.map(\.terminal) }

    /// The record of the paused Task `id`, or nil when it is not paused.
    func pausedRecord(_ id: String?) -> Paused? { paused.first { $0.id == id } }

    /// A list row's state: paused is muxa's own, not a state the agent reports.
    func taskState(_ t: Terminal) -> TaskState { pausedRecord(t.id) != nil ? .paused : t.state }

    /// Live terminals that are not a Task's: Routine runs first, then shells
    /// and terminals muxa did not start. The lists keep them collapsed. A
    /// Task's shells are listed with their Task.
    var otherTerminals: [Terminal] {
        let others = terminals.filter { $0.live && !$0.isTaskAgent && !$0.isTaskShell }
        return others.filter { $0.routineName != nil } + others.filter { $0.routineName == nil }
    }

    /// The live shells of the Task whose agent is `task` (ADR 0079), in the
    /// server's order. Only a Task's agent has them.
    func shells(of task: String?) -> [Terminal] {
        guard let agent = terminals.first(where: { $0.id == task }), agent.isTaskAgent, let dir = agent.taskDir else {
            return []
        }
        return terminals.filter { $0.live && $0.isTaskShell && $0.taskDir == dir }
    }
}

/// The section of the other terminals, as every list names it.
enum OtherTerminals {
    static let title = "Other terminals"

    static func count(_ n: Int) -> String { "\(n) terminal\(n == 1 ? "" : "s")" }
}

extension View {
    /// The control that opens and closes the other terminals.
    func otherTerminalsToggle(open: Bool) -> some View {
        accessibilityIdentifier("tasks.others")
            .accessibilityValue(open ? "open" : "closed")
    }
}
