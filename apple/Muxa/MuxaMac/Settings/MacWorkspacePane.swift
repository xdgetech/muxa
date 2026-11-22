import MuxaV2
import SwiftUI

/// One Workspace on the right of Settings: its name, root and repos, and the
/// mock's tabs. The mock's Memory tab is left out: the protocol has no data for it.
struct MacWorkspacePane: View {
    @Environment(NextModel.self) private var model
    let name: String
    @State private var tab = MacWorkspacePane.firstTab

    /// The tab a pane opens on; the debug snapshot sets it.
    @MainActor static var firstTab = WorkspaceSettings.Tab.general

    var body: some View {
        if let w = model.settings.workspace(name) {
            VStack(alignment: .leading, spacing: 0) {
                VStack(alignment: .leading, spacing: 8) {
                    Text(w.name).font(.system(size: 21, weight: .bold))
                    Text(subtitle(w)).foregroundStyle(Palette.textSecondary)
                    UnderlineTabs(tabs: tabs, selection: $tab)
                        .padding(.top, 6)
                }
                .padding(.horizontal, 28)
                .padding(.top, 18)
                content(w)
            }
        } else {
            ContentUnavailableView("This Workspace is gone", systemImage: "rectangle.stack",
                                   description: Text("It was removed, here or on the host."))
        }
    }

    private var tabs: [(title: String, tag: WorkspaceSettings.Tab, id: String)] {
        [
            ("General", .general, "ws.tab.general"),
            ("Connections", .connections, "ws.tab.connections"),
            ("MCP servers", .mcp, "ws.tab.mcp"),
            ("Agents", .agent, "ws.tab.agent"),
            ("Routines", .routines, "ws.tab.routines"),
        ]
    }

    /// The root, and the repos the host found when it said.
    private func subtitle(_ w: SettingsWorkspace) -> String {
        guard let repos = model.settings.usage[w.name]?.repos.count else { return w.root }
        return "\(w.root) · " + (repos == 1 ? "1 repo" : "\(repos) repos")
    }

    @ViewBuilder
    private func content(_ w: SettingsWorkspace) -> some View {
        switch tab {
        case .general: form { WorkspaceGeneralForm(workspace: w) }
        case .agent: form { WorkspaceAgentForm(workspace: w) }
        case .connections: page { MacConnectionsTable(workspace: w) }
        case .mcp: page { MacMcpCards(workspace: w) }
        case .routines: page { MacRoutinesTable(workspace: w) }
        }
    }

    /// The shared editable forms, on the pane at the mock's width.
    private func form(@ViewBuilder _ content: () -> some View) -> some View {
        content()
            .frame(maxWidth: MacSettingsLayout.formWidth)
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.horizontal, 8)
    }

    private func page(@ViewBuilder _ content: () -> some View) -> some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 14) {
                SettingsNotice()
                content()
            }
            .disabled(model.settings.readOnly)
            .frame(maxWidth: MacSettingsLayout.maxWidth, alignment: .leading)
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.horizontal, 28)
            .padding(.vertical, 18)
        }
    }
}

/// An uppercase header row of a settings table; nil width fills the rest.
struct MacTableHeader: View {
    let columns: [(title: String, width: CGFloat?)]

    var body: some View {
        HStack(spacing: 12) {
            ForEach(Array(columns.enumerated()), id: \.offset) { _, c in
                SectionLabel(title: c.title).cell(c.width)
            }
        }
        .padding(.horizontal, 12)
        .padding(.bottom, 6)
        .overlay(alignment: .bottom) { Rectangle().fill(Palette.border).frame(height: 1) }
    }
}

/// The outlined Edit at the end of a row; the whole row is the button.
struct MacEditChip: View {
    var body: some View {
        Text("Edit")
            .font(.system(size: 12))
            .foregroundStyle(Palette.textMid)
            .frame(maxWidth: .infinity, minHeight: 28)
            .overlay(RoundedRectangle(cornerRadius: 6).strokeBorder(Palette.borderStrong))
    }
}

/// A 46 pt table row over a divider.
struct MacTableRow<Content: View>: View {
    @ViewBuilder var content: Content

    var body: some View {
        HStack(spacing: 12) { content }
            .padding(.horizontal, 12)
            .frame(minHeight: 46)
            .overlay(alignment: .bottom) { Rectangle().fill(Palette.rowDivider).frame(height: 1) }
            .contentShape(Rectangle())
    }
}

/// `+ Add …` under a table or cards.
struct MacAddButton: View {
    let title: String
    let id: String
    let action: () -> Void

    var body: some View {
        Button(action: action) { Text(title) }
            .buttonStyle(ActionButtonStyle(kind: .secondary))
            .accessibilityIdentifier(id)
    }
}

extension View {
    /// A table cell: a fixed width, or nil for the column that fills the rest.
    func cell(_ width: CGFloat?) -> some View {
        frame(minWidth: width, maxWidth: width ?? .infinity, alignment: .leading)
    }
}
