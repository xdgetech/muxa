import MuxaSession
import SwiftUI

/// The desktop app (ADR 0070): Home and Focus on a host's muxa server over
/// protocol v2, as the iOS app. `-demo` starts the in-app demo server (UI tests).
@main
struct MuxaMacApp: App {
    @State private var model = NextModel(hosts: HostStore(file: HostStore.defaultFile))
    @State private var nav = NextNavigation()

    var body: some Scene {
        Window("Muxa", id: "main") {
            NextRoot()
                .environment(model)
                .environment(nav)
                .tint(Theme.accent)
                .frame(minWidth: 720, minHeight: 480)
                .task {
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
            CommandGroup(replacing: .newItem) {}
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
                Button("Toggle Sidebar") { nav.toggleSidebar() }
                    .keyboardShortcut("b", modifiers: .command)
                    .disabled(!nav.focus)
            }
            CommandMenu("Host") {
                Button("Disconnect") { Task { await model.disconnect() } }
                    .disabled(!model.isConnected)
            }
        }
    }
}
