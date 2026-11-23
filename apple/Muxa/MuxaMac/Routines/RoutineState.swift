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
