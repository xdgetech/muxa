import Observation
import SwiftUI

/// What the Mac's menus and its screens share beyond navigation: the
/// shortcuts sheet and the Quick Capture panel. The app sets `showCapture`.
@MainActor
@Observable
final class MacChrome {
    var showShortcuts = false
    /// Vim's window prefix was pressed and waits for its command key.
    var windowPrefixPending = false
    /// Opens the Quick Capture panel.
    var showCapture: () -> Void = {}
}
