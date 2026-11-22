import MuxaV2
import SwiftUI

/// The Routines page's groups, in the mock's order.
enum RoutineGroup: CaseIterable {
    case failed, running, paused

    var title: String {
        switch self {
        case .failed: "Failed"
        case .running: "Running"
        case .paused: "Paused"
        }
    }

    var color: Color {
        switch self {
        case .failed: Palette.late
        case .running: Palette.okText
        case .paused: Palette.textTertiary
        }
    }

    var badgeBackground: Color {
        switch self {
        case .failed: Palette.warnBack
        case .running: Palette.okBack
        case .paused: Palette.pill
        }
    }

    var badgeForeground: Color {
        switch self {
        case .failed: Palette.late
        case .running: Palette.okText
        case .paused: Palette.textSecondary
        }
    }
}

extension Routine {
    var group: RoutineGroup {
        if !enabled { return .paused }
        return last?.ok == false ? .failed : .running
    }

    /// The schedule also while paused, where `when` says only "Off".
    var timing: String {
        var on = self
        on.enabled = true
        return on.when
    }
}

/// A Unix time as the Routines page prints it.
func routineStamp(_ seconds: Int64) -> String {
    Date(timeIntervalSince1970: TimeInterval(seconds)).formatted(date: .abbreviated, time: .shortened)
}

extension NextModel {
    /// The Routine as `.muxa/routines.toml` says it; nil until Settings are read.
    func routineSettings(_ r: Routine) -> SettingsRoutine? {
        settings.workspace(r.workspace)?.routines.first { $0.name == r.name }
    }

    /// Pauses or resumes a Routine through `routine-update`, at the current revision.
    func setRoutine(_ r: Routine, enabled: Bool) async {
        guard var spec = routineSettings(r)?.spec else {
            requestError = "The settings are not loaded yet."
            return
        }
        spec.enabled = enabled
        let failure = await settings.save([.routineUpdate(workspace: r.workspace, routine: spec)])
        await refreshRoutines()
        if let failure { requestError = failure }
    }
}
