import AppKit
import MuxaSession
import SwiftTerm

/// Focus's pane commands, for the Pane menu and Vim's window keys alike. The
/// panes are the open Task's agent and its shells (ADR 0079): a split starts a
/// shell on the host, and closing a pane ends its shell. The agent's pane
/// never closes this way.
@MainActor
enum PaneCommands {
    static func run(_ command: WindowCommand, nav: NextNavigation, model: NextModel) {
        switch command {
        case .split: split(nav: nav, model: model)
        case .close:
            guard let shell = activeShell(nav: nav, model: model) else { return }
            close([shell], nav: nav, model: model)
        case .closeOthers:
            let others = model.shells(of: nav.terminal).map(\.id).filter { $0 != nav.activePane }
            close(others, nav: nav, model: model)
        case .next: nav.nextPane()
        case .previous: nav.previousPane()
        case .pane(let n): nav.focusPane(n)
        }
    }

    static func canSplit(nav: NextNavigation, model: NextModel) -> Bool {
        guard nav.focus, model.isConnected,
              let agent = model.terminals.first(where: { $0.id == nav.terminal }), agent.isTaskAgent, agent.live
        else { return false }
        return model.shells(of: agent.id).count < maxShells
    }

    /// The pane with the keyboard, when it is one of the open Task's shells.
    static func activeShell(nav: NextNavigation, model: NextModel) -> String? {
        guard nav.focus, let id = nav.activePane else { return nil }
        return model.shells(of: nav.terminal).contains { $0.id == id } ? id : nil
    }

    /// The server's limit: with the agent, four panes fit side by side.
    static let maxShells = 3

    private static func split(nav: NextNavigation, model: NextModel) {
        guard canSplit(nav: nav, model: model), let agent = nav.terminal else { return }
        Task {
            guard let shell = await model.startShell(beside: agent), nav.terminal == agent else { return }
            nav.sync(shells: model.shells(of: agent).map(\.id), focus: shell.id)
        }
    }

    private static func close(_ shells: [String], nav: NextNavigation, model: NextModel) {
        guard !shells.isEmpty, let agent = nav.terminal else { return }
        Task {
            for shell in shells { await model.closeShell(shell) }
            if nav.terminal == agent { nav.sync(shells: model.shells(of: agent).map(\.id)) }
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
