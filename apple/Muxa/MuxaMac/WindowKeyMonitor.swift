import AppKit
import MuxaSession
import SwiftTerm

/// Focus's pane commands, for the Pane menu and Vim's window keys alike.
@MainActor
enum PaneCommands {
    static func run(_ command: WindowCommand, nav: NextNavigation, model: NextModel) {
        switch command {
        case .split:
            nav.focus = true
            nav.split(from: model.needsYou + model.liveTasks + model.otherTerminals)
        case .close: nav.closePane()
        case .closeOthers: nav.closeOtherPanes()
        case .next: nav.nextPane()
        case .previous: nav.previousPane()
        case .pane(let n): nav.focusPane(n)
        }
    }
}

/// Vim's window keys (ADR 0078): while a terminal in Focus has the keyboard,
/// the user's prefix and the key after it run a pane command. Off until the
/// user picks a prefix in Settings.
@MainActor
final class WindowKeyMonitor {
    private let model: NextModel
    private let nav: NextNavigation
    private let chrome: MacChrome
    private let bindings: KeyBindings
    private var keys = WindowKeys()
    private var monitor: Any?

    init(model: NextModel, nav: NextNavigation, chrome: MacChrome, bindings: KeyBindings = .shared) {
        self.model = model
        self.nav = nav
        self.chrome = chrome
        self.bindings = bindings
    }

    func install() {
        guard monitor == nil else { return }
        monitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { [weak self] event in
            let chord = KeyChord(event)
            let inTerminal = event.window?.firstResponder is SwiftTerm.TerminalView
            let consumed = MainActor.assumeIsolated { self?.handle(chord, inTerminal: inTerminal) ?? false }
            return consumed ? nil : event
        }
    }

    /// True when the key is used up here and must not reach the terminal.
    private func handle(_ chord: KeyChord?, inTerminal: Bool) -> Bool {
        guard let prefix = bindings.vimPrefix, nav.focus, inTerminal, let chord else {
            setPending(false)
            return false
        }
        let outcome = keys.press(chord, prefix: prefix)
        setPending(keys.pending)
        switch outcome {
        case .pass: return false
        case .consume: return true
        case .run(let command):
            PaneCommands.run(command, nav: nav, model: model)
            return true
        }
    }

    private func setPending(_ pending: Bool) {
        if !pending { keys.cancel() }
        if chrome.windowPrefixPending != pending { chrome.windowPrefixPending = pending }
    }
}
