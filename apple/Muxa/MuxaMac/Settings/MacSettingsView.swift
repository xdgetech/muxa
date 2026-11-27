import AppKit
import MuxaV2
import SwiftUI

/// What the Settings window shows on its right.
enum MacSettingsPlace: Hashable {
    case workspace(String)
    case workers
    case assistant
    case build
    case host
    case connection
    case keyboard
    case terminal
}

/// The Settings window (`⌘,`) as in `Settings.dc.html`: a header, the
/// Workspaces and the host on the left, the picked one on the right.
struct MacSettingsView: View {
    @Environment(NextModel.self) private var model
    @Environment(NextNavigation.self) private var nav
    @Environment(\.openWindow) private var openWindow
    @State private var picked: MacSettingsPlace? = Self.firstPlace
    /// The place Settings opens on; snapshots set it.
    static var firstPlace: MacSettingsPlace?

    var body: some View {
        VStack(spacing: 0) {
            header
            HStack(spacing: 0) {
                MacSettingsSidebar(current: current) { picked = $0 }
                    .column(width: 260)
                detail
                    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
                    .background(Palette.pane)
                    .environment(\.formGround, Palette.pane)
            }
        }
        .font(.system(size: 13))
        .foregroundStyle(Palette.text)
        .background(Palette.ground)
        .tint(Palette.accent)
        .task(id: model.isConnected) { if model.isConnected { await model.settings.load() } }
        .onChange(of: nav.settingsRequest, initial: true) { _, place in
            // The host summary asks for its page; the window may already be open.
            guard place == .host else { return }
            picked = .host
            nav.settingsRequest = nil
        }
    }

    /// The picked place while it still exists, else the first Workspace.
    private var current: MacSettingsPlace {
        let workspaces = model.settings.settings?.workspaces ?? []
        switch picked {
        case .workspace(let name) where workspaces.contains(where: { $0.name == name }): return .workspace(name)
        case .workers where model.settings.settings != nil: return .workers
        case .assistant where model.settings.settings != nil: return .assistant
        case .build where model.settings.settings?.build != nil: return .build
        case .host where model.isConnected: return .host
        case .connection: return .connection
        case .keyboard: return .keyboard
        case .terminal: return .terminal
        default: return workspaces.first.map { .workspace($0.name) } ?? .connection
        }
    }

    private var header: some View {
        MacHeaderBar {
            Button(action: goHome) {
                HStack(spacing: 6) {
                    Image(systemName: "chevron.left").font(.system(size: 12, weight: .semibold))
                    Text("Home")
                }
                .foregroundStyle(Palette.textSecondary)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityIdentifier("settings.home")
            Text("Settings").font(.system(size: 17, weight: .bold))
            KeyHint(Shortcut.settings.display)
        } trailing: {
            EmptyView()
        }
    }

    @ViewBuilder
    private var detail: some View {
        switch current {
        case .workspace(let name): MacWorkspacePane(name: name).id(name)
        case .workers:
            VStack(alignment: .leading, spacing: 0) {
                MacPaneTitle(title: "Workers", subtitle: "Agents a Routine can run as, on this host.")
                WorkersSettings().frame(maxWidth: MacSettingsLayout.formWidth).padding(.horizontal, 8)
            }
        case .assistant:
            VStack(alignment: .leading, spacing: 0) {
                MacPaneTitle(title: "Capture assistant", subtitle: "Suggests where each capture goes, on this host.")
                AssistantSettings().frame(maxWidth: MacSettingsLayout.formWidth).padding(.horizontal, 8)
            }
        case .build:
            VStack(alignment: .leading, spacing: 0) {
                MacPaneTitle(title: "Build disk", subtitle: "Where Task worktrees put their build output, on this host.")
                BuildSettings().frame(maxWidth: MacSettingsLayout.formWidth).padding(.horizontal, 8)
            }
        case .host:
            VStack(alignment: .leading, spacing: 0) {
                MacPaneTitle(title: "Host resources", subtitle: "What uses this host's CPU, memory and disk.")
                HostSettings().padding(.horizontal, 12)
            }
        case .connection: MacConnectionPane()
        case .keyboard: MacKeyboardPane()
        case .terminal:
            VStack(alignment: .leading, spacing: 0) {
                MacPaneTitle(title: "Appearance", subtitle: "The theme and terminal font on this Mac.")
                TerminalSettingsForm().frame(maxWidth: MacSettingsLayout.formWidth).padding(.horizontal, 8)
            }
        }
    }

    /// Closes Settings and shows Home in the main window.
    private func goHome() {
        let settingsWindow = NSApp.keyWindow
        nav.focus = false
        nav.tab = .inbox
        openWindow(id: "main")
        settingsWindow?.close()
    }
}

enum MacSettingsLayout {
    /// The mock's content width on the right pane.
    static let maxWidth: CGFloat = 900
    /// A grouped Form's width: its rows stop at about 704 pt and it pads
    /// 20 on each side, so its edge lines up with the pane's titles.
    static let formWidth: CGFloat = 744
}

/// The title block of a pane: 21 pt bold over a secondary line.
struct MacPaneTitle: View {
    let title: String
    var subtitle: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(title).font(.system(size: 21, weight: .bold))
            if let subtitle { Text(subtitle).foregroundStyle(Palette.textSecondary) }
        }
        .padding(.horizontal, 28)
        .padding(.top, 18)
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}
