import MuxaSession
import MuxaV2
import SwiftUI

/// The app on protocol v2: the hosts until one is connected, then Home or Focus.
struct NextRoot: View {
    @Environment(NextModel.self) private var model
    @Environment(NextNavigation.self) private var nav
    @State private var editing: HostDraft?

    var body: some View {
        switch model.link {
        case .connected, .reconnecting:
            Group {
                if nav.focus {
                    FocusView(selected: Bindable(nav).terminal) { nav.focus = false }
                } else {
                    HomeView(openTerminal: nav.open, showFocus: { nav.focus = true })
                }
            }
            .sheet(item: Bindable(nav).newTask) { seed in
                StartTaskSheet(seed: seed, onClose: nav.taskStarted)
            }
            .background { modeKeys }
            .sheet(isPresented: Bindable(nav).showPalette) { GoToTask() }
        default:
            connect
        }
    }

    /// `⌘⇧H` and `⌘⇧F`, as on the desktop (ADR 0070). The Mac has them in its Go menu.
    private var modeKeys: some View {
        Group {
            #if os(iOS)
            Button("") { nav.focus = false }.keyboardShortcut("h", modifiers: [.command, .shift])
            Button("") { nav.focus = true }.keyboardShortcut("f", modifiers: [.command, .shift])
            Button("") { nav.showPalette = true }.keyboardShortcut("p", modifiers: .command)
            #endif
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
                #if os(macOS)
                if model.serverIsLocal {
                    Section {
                        Button("This Mac") { Task { await model.connectLocal() } }
                            .accessibilityIdentifier("next.local")
                    } footer: {
                        Text("The muxa server runs here. The app reaches it without SSH.")
                    }
                }
                #endif
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
