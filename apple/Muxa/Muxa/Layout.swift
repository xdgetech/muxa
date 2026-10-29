import SwiftUI

extension EnvironmentValues {
    /// The phone layout: compact in either direction, never decided by the device
    /// (ADR 0012). A Max iPhone in landscape is regular wide but compact tall; a
    /// folded iPhone Duo is compact, an unfolded one is not.
    var phoneLayout: Bool {
        horizontalSizeClass == .compact || verticalSizeClass == .compact
    }
}
