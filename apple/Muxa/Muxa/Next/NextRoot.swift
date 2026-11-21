import MuxaSession
import MuxaV2
import SwiftUI

/// The app on protocol v2, started with `-next` until it replaces the v1 screens.
/// Step 1 of D4: connecting; Home and Focus follow.
struct NextRoot: View {
    @Environment(NextModel.self) private var model

    var body: some View {
        switch model.link {
        case .connected, .reconnecting:
            HomeView()
        default:
            connect
        }
    }

    private var connect: some View {
        NavigationStack {
            List {
                Section {
                    Text(status).accessibilityIdentifier("next.link")
                    if model.untrustedKey != nil {
                        Button("Trust This Host") { Task { await model.trust() } }
                    }
                }
                if model.isConnected {
                    Section("Connected") {
                        Text("\(model.inbox.items.count) in the Inbox")
                            .accessibilityIdentifier("next.inbox.count")
                        Text("\(model.needsYou.count) need you")
                            .accessibilityIdentifier("next.needsyou.count")
                        Text("\(model.routines.count) Routines")
                    }
                } else {
                    Section("Hosts") {
                        ForEach(model.hosts.hosts) { host in
                            Button(host.title) { Task { await model.connect(to: host.id) } }
                        }
                        Button("Try the Demo") { Task { await model.startDemo() } }
                            .accessibilityIdentifier("next.demo")
                    }
                }
            }
            .navigationTitle("muxa")
        }
    }

    private var status: String {
        switch model.link {
        case .offline: "Not connected"
        case .connecting: "Connecting…"
        case .connected: model.demo ? "Connected to the demo" : "Connected"
        case .reconnecting(let why): "Reconnecting: \(why)"
        case .failed(let why): why
        }
    }
}
