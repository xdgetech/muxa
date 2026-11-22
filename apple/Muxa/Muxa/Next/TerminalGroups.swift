import MuxaV2
import SwiftUI

/// Which terminals the Task lists show, and where. Ended terminals are left
/// out: a failed agent already has a Failed item in the Inbox.
extension NextModel {
    /// The live Tasks' agents.
    var liveTasks: [Terminal] { terminals.filter { $0.live && $0.isTaskAgent } }

    /// Live terminals that are not a Task's agent: Routine runs first, then
    /// shells and terminals muxa did not start. The lists keep them collapsed.
    var otherTerminals: [Terminal] {
        let others = terminals.filter { $0.live && !$0.isTaskAgent }
        return others.filter { $0.routineName != nil } + others.filter { $0.routineName == nil }
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
