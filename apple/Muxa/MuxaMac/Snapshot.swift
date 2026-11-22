#if DEBUG
import AppKit
import SwiftUI

/// Debug builds only: with `-demo`, `MUXA_SNAPSHOT=<png>` and optionally
/// `MUXA_SCREEN=home|focus|routines|shortcuts` in the environment, the app
/// draws its main window into the PNG and quits, so a change can be compared
/// with the mocks without screen-recording permission. Environment, not
/// arguments: macOS opens a path argument as a document and shows no window.
@MainActor
enum Snapshot {
    static func runIfAsked(nav: NextNavigation, chrome: MacChrome) async {
        let env = ProcessInfo.processInfo.environment
        guard let path = env["MUXA_SNAPSHOT"] else { return }
        switch env["MUXA_SCREEN"] {
        case "focus": nav.focus = true
        case "routines": nav.tab = .routines
        case "shortcuts": chrome.showShortcuts = true
        default: break
        }
        // The mocks' desktop frame.
        NSApp.windows.first { $0.isVisible && !($0 is NSPanel) }?.setContentSize(NSSize(width: 1440, height: 900))
        try? await Task.sleep(for: .seconds(2))
        let window = NSApp.windows.first { $0.attachedSheet != nil } ?? NSApp.windows.first { $0.isVisible && !($0 is NSPanel) }
        let target = window?.attachedSheet ?? window
        if let view = target?.contentView?.superview ?? target?.contentView,
           let rep = view.bitmapImageRepForCachingDisplay(in: view.bounds) {
            view.cacheDisplay(in: view.bounds, to: rep)
            try? rep.representation(using: .png, properties: [:])?.write(to: URL(fileURLWithPath: path))
        }
        exit(0)
    }
}
#endif
