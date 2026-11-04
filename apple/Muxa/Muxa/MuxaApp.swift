import MuxaUI
import SwiftUI

@main
struct MuxaApp: App {
    @State private var model = AppModel()
    @Environment(\.scenePhase) private var scenePhase

    init() {
        #if DEBUG
        // The phone screenshots: no on-screen keyboard over the pane.
        if ProcessInfo.processInfo.arguments.contains("-showcase"), UIDevice.current.userInterfaceIdiom == .phone {
            KeyInputView.waitsForTap = true
        }
        #endif
    }

    @ViewBuilder
    private var root: some View {
        #if DEBUG
        if FoldHarness<RootView>.enabled {
            FoldHarness { RootView() }
        } else {
            RootView()
        }
        #else
        RootView()
        #endif
    }

    var body: some Scene {
        WindowGroup {
            root
                .environment(model)
                .tint(Theme.accent)
                .task { await model.resume() }
        }
        .onChange(of: scenePhase) { _, phase in
            Task {
                switch phase {
                case .active: await model.resume()
                // iOS suspends the app soon after; leave cleanly so the bridge exits.
                case .background: await model.suspend()
                default: break
                }
            }
        }
        .commands { PaneCommands(model: model) }
    }
}

/// Only Cmd chords are taken; every other key goes to the pane.
struct PaneCommands: Commands {
    let model: AppModel

    var body: some Commands {
        CommandMenu("Panes") {
            Button("Switch Session…") { model.showSwitcher = true }
                .keyboardShortcut("k")
            Button("New Task…") { model.startNewTask() }
                .keyboardShortcut("n")
                .disabled(!model.canCreateTasks)
            Button("Agent / Terminal") { Task { await model.toggleAgentTerminal() } }
                .keyboardShortcut("j")
                .disabled(!model.showsPane || model.counterpart == nil)
            Button("Pretty / Raw") { model.setPretty(!model.showsPretty) }
                .keyboardShortcut("r")
                .disabled(!model.showsPane || !model.canShowPretty)
            Button("Documents…") { model.openReader() }
                .keyboardShortcut("d", modifiers: [.command, .shift])
                .disabled(!model.showsPane || !model.canReadDocs)
            Button("Previous Session") { Task { await model.step(-1) } }
                .keyboardShortcut("[", modifiers: [.command, .shift])
                .disabled(!model.isConnected)
            Button("Next Session") { Task { await model.step(1) } }
                .keyboardShortcut("]", modifiers: [.command, .shift])
                .disabled(!model.isConnected)
            Divider()
            ForEach(1..<10) { n in
                Button("Session \(n)") { Task { await model.openSession(number: n) } }
                    .keyboardShortcut(KeyEquivalent(Character("\(n)")))
                    .disabled(!model.isConnected)
            }
        }
        // The sidebar's modes (ADR 0035 §7). The item's own keys are on its screen.
        CommandMenu("Go") {
            Button("Inbox") { model.showMode(.inbox) }
                .keyboardShortcut("t")
            Button("Tasks") { model.showMode(.tasks) }
                .keyboardShortcut("l")
            Button("Sessions") { model.showMode(.sessions) }
                .keyboardShortcut("s", modifiers: [.command, .shift])
            Button("Show or Hide the Sidebar") { model.toggleSidebar() }
                .keyboardShortcut("0")
        }
        // The system's own Settings item: Cmd+comma belongs to it.
        CommandGroup(replacing: .appSettings) {
            Button("Settings…") { model.showSettings = true }
                .keyboardShortcut(",")
        }
    }
}
