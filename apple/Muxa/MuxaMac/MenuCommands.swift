import AppKit
import MuxaSession
import MuxaV2
import SwiftUI

// The menu bar's items as views. A View's body tracks the @Observable model
// and navigation it reads, so `.disabled` and the titles follow them; read
// directly in the App's `commands`, they were computed once (ADR 0070, 0078).

/// File → Close: the active shell in Focus, else the key window.
struct CloseItem: View {
    let model: NextModel
    let nav: NextNavigation
    let chrome: MacChrome

    var body: some View {
        Button(PaneCommands.activeShell(nav: nav, model: model) != nil ? "Close Shell" : "Close") { close() }
            .shortcut(.closePane)
    }

    /// ⌘W: in Focus it closes the active shell and never the window, so a
    /// stray ⌘W on the agent does nothing. Elsewhere it closes the key window.
    private func close() {
        let key = NSApp.keyWindow
        guard nav.focus, let key, key === chrome.mainWindow else { return key?.performClose(nil) ?? () }
        if PaneCommands.activeShell(nav: nav, model: model) != nil {
            PaneCommands.run(.close, nav: nav, model: model)
        } else {
            NSSound.beep()
        }
    }
}

/// File → Capture, Capture from Anywhere and New Task.
struct CaptureItems: View {
    let model: NextModel
    let nav: NextNavigation
    let showCapture: () -> Void

    var body: some View {
        Button("Capture…", action: showCapture)
            .shortcut(.capture)
            .disabled(!model.isConnected)
        Button("Capture from Anywhere", action: showCapture)
            .shortcut(.captureAnywhere)
        Button("New Task…") { nav.newTask = TaskSeed() }
            .shortcut(.newTask)
            .disabled(!model.isConnected)
    }
}

/// The Go menu: the three places, the Tasks that need the user, the panels.
struct GoMenuItems: View {
    let model: NextModel
    let nav: NextNavigation

    var body: some View {
        Button("Home") { nav.focus = false; nav.tab = .inbox }
            .shortcut(.home)
        Button("Focus") { nav.focus = true }
            .shortcut(.focus)
        // Never disabled, like Home and Focus: Bots shows its own state when not connected,
        // and a disabled key here did nothing until the menu was opened once.
        Button("Bots") { nav.focus = false; nav.tab = .bots }
            .shortcut(.bots)
        Button("Routines in Bots") { nav.focus = false; nav.tab = .bots }
            .shortcut(.routines)
        Divider()
        Button("Next Task That Needs You") { openNext(in: model.needsYou) }
            .shortcut(.nextNeedsYou)
            .disabled(model.needsYou.isEmpty)
        Button("Next Task") { openNext(in: model.needsYou + model.liveTasks) }
            .shortcut(.nextTask)
            .disabled(model.liveTasks.isEmpty && model.needsYou.isEmpty)
        Button("Previous Task That Needs You") { openNext(in: model.needsYou, step: -1) }
            .shortcut(.previousNeedsYou)
            .disabled(model.needsYou.isEmpty)
        Button("Previous Task") { openNext(in: model.needsYou + model.liveTasks, step: -1) }
            .shortcut(.previousTask)
            .disabled(model.liveTasks.isEmpty && model.needsYou.isEmpty)
        Divider()
        Button(nav.showInbox ? "Hide Inbox Panel" : "Show Inbox Panel") { nav.showInbox.toggle() }
            .shortcut(.inboxPanel)
            .disabled(!nav.focus)
        Button(nav.showReader ? "Hide Reader Panel" : "Show Reader Panel") { nav.showReader.toggle() }
            .shortcut(.readerPanel)
            .disabled(!nav.focus)
        Button(nav.showChat ? "Hide Chat Panel" : "Show Chat Panel") { nav.showChat.toggle() }
            .shortcut(.chatPanel)
            .disabled(!nav.focus)
        Button("Toggle Sidebar") { nav.toggleSidebar() }
            .shortcut(.sidebar)
            .disabled(!nav.focus)
        Divider()
        Button("Go to Task…") { nav.showPalette = true }
            .shortcut(.goToTask)
            .disabled(!model.isConnected)
    }

    /// Opens the Task after (or with `step` -1, before) the one Focus shows,
    /// from `list`. It wraps around, so each press moves on.
    private func openNext(in list: [Terminal], step: Int = 1) {
        let unique = list.reduce(into: [Terminal]()) { if !$0.contains($1) { $0.append($1) } }
        guard !unique.isEmpty else { return }
        let at = unique.firstIndex { $0.id == nav.terminal } ?? (step > 0 ? -1 : 0)
        nav.open(unique[(at + step + unique.count) % unique.count])
    }
}

/// The Pane menu: Focus's panes, and in Bots the page's tabs on `⌘1`…`⌘5`.
struct PaneMenuItems: View {
    let model: NextModel
    let nav: NextNavigation
    let chrome: MacChrome

    var body: some View {
        // The keys work in Focus only, so Home keeps them for its own use.
        Button("Split Pane") { pane(.split) }
            .shortcut(.split)
            .disabled(!PaneCommands.canSplit(nav: nav, model: model))
        Button("Next Pane") { pane(.next) }
            .shortcut(.nextPane)
            .disabled(!nav.focus || nav.panes.count < 2)
        Button("Previous Pane") { pane(.previous) }
            .shortcut(.previousPane)
            .disabled(!nav.focus || nav.panes.count < 2)
        Button("Close Other Shells") { pane(.closeOthers) }
            .shortcut(.closeOtherPanes)
            .disabled(!nav.focus || nav.panes.count < 2)
        Divider()
        Button("Close Task…") { chrome.closingTask = focusedTask }
            .shortcut(.closeTask)
            .disabled(focusedTask == nil)
        Divider()
        // In Bots the first keys pick the Bot's tab instead (ADR 0086).
        ForEach(1...9, id: \.self) { n in
            Button(inBots ? (BotTab(rawValue: n)?.title ?? "Pane \(n)") : "Pane \(n)") {
                if inBots, let tab = BotTab(rawValue: n) { chrome.botTab = tab } else { pane(.pane(n)) }
            }
            .keyboardShortcut(KeyEquivalent(Character("\(n)")), modifiers: .command)
            .disabled(inBots ? BotTab(rawValue: n) == nil : n > nav.panes.count)
        }
    }

    /// The Bots page shows: `⌘1`… are its tabs.
    private var inBots: Bool { !nav.focus && (nav.tab == .bots || nav.tab == .routines) }

    private func pane(_ command: WindowCommand) {
        PaneCommands.run(command, nav: nav, model: model)
    }

    /// The Task Focus shows, while its agent runs.
    private var focusedTask: Terminal? {
        guard nav.focus else { return nil }
        return model.terminals.first { $0.id == nav.terminal && $0.isTaskAgent }
    }
}

/// The Host menu.
struct HostMenuItems: View {
    let model: NextModel

    var body: some View {
        Button("Disconnect") { Task { await model.disconnect() } }
            .disabled(!model.isConnected)
    }
}
