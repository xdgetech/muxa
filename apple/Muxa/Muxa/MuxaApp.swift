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
            Button("Inbox…") { model.showTasks.toggle() }
                .keyboardShortcut("t")
                .disabled(!model.canKeepTasks)
            Button("Agent / Terminal") { Task { await model.toggleAgentTerminal() } }
                .keyboardShortcut("j")
                .disabled(model.showTasks || model.counterpart == nil)
            Button("Pretty / Raw") { model.setPretty(!model.showsPretty) }
                .keyboardShortcut("r")
                .disabled(model.showTasks || !model.canShowPretty)
            Button("Documents…") { model.openReader() }
                .keyboardShortcut("d", modifiers: [.command, .shift])
                .disabled(model.showTasks || !model.canReadDocs)
            Button("Previous Session") { Task { await model.step(-1) } }
                .keyboardShortcut("[", modifiers: [.command, .shift])
                .disabled(model.showTasks)
            Button("Next Session") { Task { await model.step(1) } }
                .keyboardShortcut("]", modifiers: [.command, .shift])
                .disabled(model.showTasks)
            Divider()
            ForEach(1..<10) { n in
                Button("Session \(n)") { Task { await model.openSession(number: n) } }
                    .keyboardShortcut(KeyEquivalent(Character("\(n)")))
                    .disabled(model.showTasks)
            }
        }
    }
}
