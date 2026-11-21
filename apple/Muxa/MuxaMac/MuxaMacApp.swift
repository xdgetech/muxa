import MuxaSession
import SwiftUI

/// The desktop app (ADR 0070): Home and Focus on a host's muxa server over
/// protocol v2, as the iOS app. `-demo` starts the in-app demo server (UI tests).
@main
struct MuxaMacApp: App {
    @State private var model = NextModel(hosts: HostStore(file: HostStore.defaultFile))

    var body: some Scene {
        Window("Muxa", id: "main") {
            NextRoot()
                .environment(model)
                .tint(Theme.accent)
                .frame(minWidth: 720, minHeight: 480)
                .task {
                    if ProcessInfo.processInfo.arguments.contains("-demo") {
                        await model.startDemo()
                    } else if let last = model.hosts.lastUsed {
                        await model.connect(to: last)
                    }
                }
        }
        .commands {
            CommandGroup(replacing: .newItem) {}
            CommandMenu("Host") {
                Button("Disconnect") { Task { await model.disconnect() } }
                    .disabled(!model.isConnected)
            }
        }
    }
}
