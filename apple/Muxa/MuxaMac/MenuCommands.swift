import AppKit
import MuxaSession
import MuxaV2
import SwiftUI

// The menu bar's items as views. A View's body tracks the @Observable model
// and navigation it reads, so the titles follow them; read directly in the
// App's `commands`, they were computed once (ADR 0070, 0078).

/// Runs `action` when `allowed`, else beeps. The Go and Pane menus check here
/// instead of `.disabled`: an item kept its first disabled state until
/// its menu was opened, so after Home its key did nothing in Focus.
@MainActor
private func when(_ allowed: Bool, _ action: () -> Void) {
    if allowed { action() } else { NSSound.beep() }
}

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
        // Bots shows its own state when not connected.
        Button("Bots") { nav.focus = false; nav.tab = .bots }
            .shortcut(.bots)
        Button("Routines in Bots") { nav.focus = false; nav.tab = .bots }
            .shortcut(.routines)
        Divider()
        Button("Next Task That Needs You") { openNext(in: model.needsYou) }
            .shortcut(.nextNeedsYou)
        Button("Next Task") { openNext(in: model.needsYou + model.liveTasks) }
            .shortcut(.nextTask)
        Button("Previous Task That Needs You") { openNext(in: model.needsYou, step: -1) }
            .shortcut(.previousNeedsYou)
        Button("Previous Task") { openNext(in: model.needsYou + model.liveTasks, step: -1) }
            .shortcut(.previousTask)
        Divider()
        Button(nav.showInbox ? "Hide Inbox Panel" : "Show Inbox Panel") { when(nav.focus) { nav.showInbox.toggle() } }
            .shortcut(.inboxPanel)
        Button(nav.showReader ? "Hide Reader Panel" : "Show Reader Panel") { when(nav.focus) { nav.showReader.toggle() } }
            .shortcut(.readerPanel)
        Button(nav.showChat ? "Hide Chat Panel" : "Show Chat Panel") { when(nav.focus) { nav.showChat.toggle() } }
            .shortcut(.chatPanel)
        Button("Toggle Sidebar") { when(nav.focus) { nav.toggleSidebar() } }
            .shortcut(.sidebar)
        Divider()
        Button("Go to Task…") { when(model.isConnected) { nav.showPalette = true } }
            .shortcut(.goToTask)
    }

    /// Opens the Task after (or with `step` -1, before) the one Focus shows,
    /// from `list`. It wraps around, so each press moves on.
    private func openNext(in list: [Terminal], step: Int = 1) {
        let unique = list.reduce(into: [Terminal]()) { if !$0.contains($1) { $0.append($1) } }
        guard !unique.isEmpty else { return NSSound.beep() }
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
        Button("Split Pane") { when(PaneCommands.canSplit(nav: nav, model: model)) { pane(.split) } }
            .shortcut(.split)
        Button("Next Pane") { when(severalPanes) { pane(.next) } }
            .shortcut(.nextPane)
        Button("Previous Pane") { when(severalPanes) { pane(.previous) } }
            .shortcut(.previousPane)
        Button("Close Other Shells") { when(severalPanes) { pane(.closeOthers) } }
            .shortcut(.closeOtherPanes)
        Divider()
        Button("Close Task…") { when(focusedTask != nil) { chrome.closingTask = focusedTask } }
            .shortcut(.closeTask)
        Divider()
        // In Bots the first keys pick the Bot's tab instead (ADR 0086).
        ForEach(1...9, id: \.self) { n in
            Button(inBots ? (BotTab(rawValue: n)?.title ?? "Pane \(n)") : "Pane \(n)") {
                if !inBots {
                    when(nav.focus && n <= nav.panes.count) { pane(.pane(n)) }
                } else if let tab = BotTab(rawValue: n) {
                    chrome.botTab = tab
                } else {
                    NSSound.beep()
                }
            }
            .keyboardShortcut(KeyEquivalent(Character("\(n)")), modifiers: .command)
        }
    }

    private var severalPanes: Bool { nav.focus && nav.panes.count > 1 }

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
