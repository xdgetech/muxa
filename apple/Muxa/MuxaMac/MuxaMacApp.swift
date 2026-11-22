import MuxaSession
import SwiftUI

/// The desktop app (ADR 0070): Home and Focus on a host's muxa server over
/// protocol v2, as the iOS app. `-demo` starts the in-app demo server (UI tests).
@main
struct MuxaMacApp: App {
    @State private var model = NextModel(hosts: HostStore(file: HostStore.defaultFile))
    @State private var nav = NextNavigation()
    @State private var quickCapture: QuickCapture?

    var body: some Scene {
        Window("Muxa", id: "main") {
            NextRoot()
                .environment(model)
                .environment(nav)
                .tint(Theme.accent)
                .frame(minWidth: 720, minHeight: 480)
                .containerBackground(Palette.ground, for: .window)
                .toolbarBackground(Palette.ground, for: .windowToolbar)
                .task {
                    if quickCapture == nil {
                        let capture = QuickCapture(model: model, nav: nav)
                        capture.register()
                        quickCapture = capture
                    }
                    if ProcessInfo.processInfo.arguments.contains("-demo") {
                        await model.startDemo()
                    } else if model.prefersLocal, model.serverIsLocal {
                        await model.connectLocal()
                    } else if let last = model.hosts.lastUsed {
                        await model.connect(to: last)
                    }
                }
        }
        .commands {
            CommandGroup(replacing: .newItem) {
                Button("Quick Capture") { quickCapture?.show() }
                    .keyboardShortcut(.space, modifiers: [.control, .option])
                Button("New Task…") { nav.newTask = TaskSeed() }
                    .keyboardShortcut("n", modifiers: [.command, .shift])
                    .disabled(!model.isConnected)
            }
            // The keys of docs/next.md, which follow VS Code (ADR 0070 decision 2).
            CommandMenu("Go") {
                Button("Home") { nav.focus = false }
                    .keyboardShortcut("h", modifiers: [.command, .shift])
                Button("Focus") { nav.focus = true }
                    .keyboardShortcut("f", modifiers: [.command, .shift])
                Divider()
                Button(nav.showInbox ? "Hide Inbox Panel" : "Show Inbox Panel") { nav.showInbox.toggle() }
                    .keyboardShortcut("i", modifiers: [.command, .shift])
                    .disabled(!nav.focus)
                Button(nav.showReader ? "Hide Reader Panel" : "Show Reader Panel") { nav.showReader.toggle() }
                    .keyboardShortcut("r", modifiers: [.command, .shift])
                    .disabled(!nav.focus)
                Button(nav.showChat ? "Hide Chat Panel" : "Show Chat Panel") { nav.showChat.toggle() }
                    .keyboardShortcut("c", modifiers: [.command, .shift])
                    .disabled(!nav.focus)
                Button("Toggle Sidebar") { nav.toggleSidebar() }
                    .keyboardShortcut("b", modifiers: .command)
                    .disabled(!nav.focus)
                Divider()
                Button("Go to Task…") { nav.showPalette = true }
                    .keyboardShortcut("p", modifiers: .command)
                    .disabled(!model.isConnected)
            }
            CommandMenu("Pane") {
                Button("Split Pane") {
                    nav.focus = true
                    nav.split(from: model.needsYou + model.terminals.filter(\.live))
                }
                .keyboardShortcut("\\", modifiers: .command)
                .disabled(!model.isConnected || nav.panes.count >= NextNavigation.maxPanes)
                Button("Close Pane") { nav.closePane() }
                    .disabled(nav.panes.count < 2)
                Divider()
                ForEach(1...9, id: \.self) { n in
                    Button("Pane \(n)") { nav.focusPane(n) }
                        .keyboardShortcut(KeyEquivalent(Character("\(n)")), modifiers: .command)
                        .disabled(n > nav.panes.count)
                }
            }
            CommandMenu("Host") {
                Button("Disconnect") { Task { await model.disconnect() } }
                    .disabled(!model.isConnected)
            }
        }
    }
}
