import Foundation
import MuxaProtocol
import MuxaSession

/// Routines in Tasks (ADR 0026).
extension AppModel {
    func routine(workspace: String, name: String) -> RoutineInfo? {
        session.routines.first { $0.workspace == workspace && $0.name == name }
    }

    /// The session of a Routine's open run, by its `taskId`.
    func runSession(_ routine: RoutineInfo) -> Session? {
        guard let task = routine.running?.task else { return nil }
        return session.sessions.first { $0.taskId == task && $0.defaultPane != nil }
    }
}

extension UInt64 {
    /// Epoch seconds as a short local date and time.
    var shownDate: String {
        Date(timeIntervalSince1970: TimeInterval(self)).formatted(date: .abbreviated, time: .shortened)
    }
}
