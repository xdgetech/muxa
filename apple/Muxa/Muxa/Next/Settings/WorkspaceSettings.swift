import MuxaV2
import SwiftUI

/// One Workspace's settings, in the mock's tabs.
struct WorkspaceSettings: View {
    @Environment(NextModel.self) private var model
    let name: String
    @State private var tab = Tab.general

    enum Tab: Hashable { case general, agent, connections, mcp, routines, bots }

    var body: some View {
        Group {
            if let w = model.settings.workspace(name) {
                VStack(spacing: 0) {
                    SegmentBar(segments: segments, selection: $tab)
                        .padding(.horizontal, 16)
                        .padding(.vertical, 10)
                    form(w)
                }
            } else {
                ContentUnavailableView("This Workspace is gone", systemImage: "rectangle.stack",
                                       description: Text("It was removed, here or on the host."))
            }
        }
        .background(Palette.ground)
        .navigationTitle(name)
        .inlineTitle()
    }

    private var segments: [Segment<Tab>] {
        [
            Segment(value: .general, title: "General", id: "ws.tab.general"),
            Segment(value: .agent, title: "Agent", id: "ws.tab.agent"),
            Segment(value: .connections, title: "Connections", id: "ws.tab.connections"),
            Segment(value: .mcp, title: "MCP", id: "ws.tab.mcp"),
            Segment(value: .routines, title: "Routines", id: "ws.tab.routines"),
            Segment(value: .bots, title: "Bots", id: "ws.tab.bots"),
        ]
    }

    @ViewBuilder
    private func form(_ w: SettingsWorkspace) -> some View {
        switch tab {
        case .general: WorkspaceGeneralForm(workspace: w)
        case .agent: WorkspaceAgentForm(workspace: w)
        case .connections: WorkspaceConnectionsForm(workspace: w)
        case .mcp: WorkspaceMcpForm(workspace: w)
        case .routines: WorkspaceRoutinesForm(workspace: w)
        case .bots: WorkspaceBotsForm(workspace: w)
        }
    }
}

/// One page of Settings: the notice, then its sections, locked while the
/// host is. Sheets go on the page: a sheet on a Form's row does not open.
struct SettingsPage<Content: View>: View {
    @Environment(NextModel.self) private var model
    @ViewBuilder let content: Content

    var body: some View {
        PaletteForm {
            SettingsNotice()
            content
        }
        .formStyle(.grouped)
        .disabled(model.settings.readOnly)
    }
}

/// The note on the Connections and MCP tabs (ADR 0075).
struct EveryAgentNote: View {
    var body: some View {
        Label {
            Text("Every agent of this Workspace gets all its connections and MCP servers: each Task, restart and Routine run. "
                + "Listing any MCP server makes MCP strict: Claude Code then loads only these servers.")
        } icon: {
            Image(systemName: "lock")
        }
        .font(.callout)
        .foregroundStyle(Palette.textSecondary)
        .accessibilityIdentifier("ws.note")
    }
}
