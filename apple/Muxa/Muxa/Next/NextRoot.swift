import MuxaSession
import MuxaV2
import SwiftUI

/// The app on protocol v2: the hosts until one is connected, then the
/// platform's `ConnectedRoot` (the touch layouts, or the Mac's Home and Focus).
struct NextRoot: View {
    @Environment(NextModel.self) private var model
    @Environment(NextNavigation.self) private var nav
    @State private var editing: HostDraft?

    var body: some View {
        Group {
            switch model.link {
            case .connected, .reconnecting:
                ConnectedRoot()
                    .sheet(item: Bindable(nav).newTask) { seed in
                        StartTaskSheet(seed: seed, onClose: nav.taskStarted)
                    }
                    .background { modeKeys }
                    #if os(macOS)
                    .overlay { if nav.showPalette { PaletteOverlay() } }
                    #else
                    .sheet(isPresented: Bindable(nav).showPalette) { GoToTask() }
                    #endif
            default:
                #if os(macOS)
                MacConnectView()
                #else
                connect
                #endif
            }
        }
        .background(Palette.ground)
        .preferredColorScheme(ThemeStore.shared.colorScheme)
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

    #if os(iOS)
    private var connect: some View {
        NavigationStack {
            List {
                Section {
                    Text(model.linkStatus).accessibilityIdentifier("next.link")
                }
                .listRowBackground(Palette.card)
                HostsSection(editing: $editing)
                Section {
                    Button("Try the Demo") { Task { await model.startDemo() } }
                        .accessibilityIdentifier("next.demo")
                } footer: {
                    Text("The demo runs in the app. Nothing reaches a host.")
                }
                .listRowBackground(Palette.card)
            }
            .groundList()
            .navigationTitle("muxa")
            .sheet(item: $editing) { NextHostEditor(draft: $0) }
            .sheet(item: Bindable(model).untrustedKey) { NextTrustSheet(key: $0) }
        }
    }
    #endif
}

extension NextModel {
    /// The connection's state, or why it failed: the not-connected screens show it.
    var linkStatus: String {
        switch link {
        case .offline: "Not connected"
        case .connecting: addingKey ? "Adding this device's key…" : "Connecting…"
        case .connected: demo ? "Connected to the demo" : "Connected"
        case .reconnecting(let why): "Reconnecting: \(why)"
        case .failed(let why): why
        }
    }
}

#if os(macOS)
/// Go to Task floating over the window, near its top, as VS Code shows quick
/// open. A click outside closes it.
private struct PaletteOverlay: View {
    @Environment(NextNavigation.self) private var nav

    var body: some View {
        ZStack(alignment: .top) {
            Color.black.opacity(0.3)
                .contentShape(Rectangle())
                .onTapGesture { nav.showPalette = false }
            GoToTask()
                .frame(width: 600)
                .clipShape(RoundedRectangle(cornerRadius: 12))
                .overlay(RoundedRectangle(cornerRadius: 12).strokeBorder(Palette.borderStrong))
                .shadow(color: .black.opacity(0.45), radius: 24, y: 8)
                .padding(.top, 72)
        }
    }
}
#endif
