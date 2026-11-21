import MuxaSession
import MuxaV2
import SwiftUI

/// The app on protocol v2, started with `-next` until it replaces the v1 screens.
/// Step 1 of D4: connecting; Home and Focus follow.
struct NextRoot: View {
    @Environment(NextModel.self) private var model
    @State private var focus = false
    @State private var terminal: String?
    @State private var editing: HostDraft?

    var body: some View {
        switch model.link {
        case .connected, .reconnecting:
            Group {
                if focus {
                    FocusView(selected: $terminal) { focus = false }
                } else {
                    HomeView(openTerminal: open, showFocus: { focus = true })
                }
            }
            .background { modeKeys }
        default:
            connect
        }
    }

    private func open(_ t: Terminal) {
        terminal = t.id
        focus = true
    }

    /// `⌘⇧H` and `⌘⇧F`, as on the desktop (ADR 0070).
    private var modeKeys: some View {
        Group {
            Button("") { focus = false }.keyboardShortcut("h", modifiers: [.command, .shift])
            Button("") { focus = true }.keyboardShortcut("f", modifiers: [.command, .shift])
        }
        .opacity(0)
        .accessibilityHidden(true)
    }

    private var connect: some View {
        NavigationStack {
            List {
                Section {
                    Text(status).accessibilityIdentifier("next.link")
                }
                Section("Hosts") {
                    ForEach(model.hosts.hosts) { host in
                        HStack {
                            Button(host.title) { Task { await model.connect(to: host.id) } }
                            Spacer()
                            Button("Edit", systemImage: "info.circle") { editing = HostDraft(host) }
                                .labelStyle(.iconOnly)
                                .buttonStyle(.borderless)
                        }
                    }
                    Button("Add Host", systemImage: "plus") { editing = HostDraft() }
                        .accessibilityIdentifier("next.addhost")
                }
                Section {
                    Button("Try the Demo") { Task { await model.startDemo() } }
                        .accessibilityIdentifier("next.demo")
                } footer: {
                    Text("The demo runs in the app. Nothing reaches a host.")
                }
            }
            .navigationTitle("muxa")
            .sheet(item: $editing) { NextHostEditor(draft: $0) }
            .sheet(item: Bindable(model).untrustedKey) { NextTrustSheet(key: $0) }
        }
    }

    private var status: String {
        switch model.link {
        case .offline: "Not connected"
        case .connecting: model.addingKey ? "Adding this device's key…" : "Connecting…"
        case .connected: model.demo ? "Connected to the demo" : "Connected"
        case .reconnecting(let why): "Reconnecting: \(why)"
        case .failed(let why): why
        }
    }
}
