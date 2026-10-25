import SwiftUI

@main
struct MuxaApp: App {
    @State private var model = AppModel()
    @Environment(\.scenePhase) private var scenePhase

    var body: some Scene {
        WindowGroup {
            RootView()
                .environment(model)
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
            Button("Agent / Terminal") { Task { await model.toggleAgentTerminal() } }
                .keyboardShortcut("j")
                .disabled(model.counterpart == nil)
            Button("Pretty / Raw") { model.setPretty(!model.showsPretty) }
                .keyboardShortcut("r")
                .disabled(!model.canShowPretty)
            Button("Previous Session") { Task { await model.step(-1) } }
                .keyboardShortcut("[", modifiers: [.command, .shift])
            Button("Next Session") { Task { await model.step(1) } }
                .keyboardShortcut("]", modifiers: [.command, .shift])
            Divider()
            ForEach(1..<10) { n in
                Button("Session \(n)") { Task { await model.openSession(number: n) } }
                    .keyboardShortcut(KeyEquivalent(Character("\(n)")))
            }
        }
    }
}
