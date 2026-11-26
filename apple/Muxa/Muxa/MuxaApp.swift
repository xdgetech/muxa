import MuxaSession
import SwiftUI
import UIKit

/// The app on protocol v2 (ADR 0066, 0067): Home and Focus on a host's muxa
/// server. `-demo` starts the in-app demo server at launch (UI tests).
@main
struct MuxaApp: App {
    @State private var model = NextModel(hosts: HostStore(file: HostStore.defaultFile))
    @State private var nav = NextNavigation()
    @Environment(\.scenePhase) private var scenePhase

    init() {
        TerminalSettings.registerFonts()
    }

    var body: some Scene {
        WindowGroup {
            NextRoot()
                .environment(model)
                .environment(nav)
                .modifier(ThemedWindow())
                .task {
                    if ProcessInfo.processInfo.arguments.contains("-demo") {
                        await model.startDemo()
                    } else if let last = model.hosts.lastUsed {
                        await model.connect(to: last)
                    }
                }
        }
        .onChange(of: scenePhase) { _, phase in
            Task {
                switch phase {
                case .active: await model.resumeIfNeeded()
                // iOS suspends the app soon after; the server keeps the session for a resume.
                case .background: await model.suspend()
                default: break
                }
            }
        }
    }
}
