import MuxaV2
import SwiftUI

/// "Allow claude.ai connectors" for a Workspace (ADR 0087): its agents then
/// load the user's claude.ai connectors beside its MCP servers.
struct ClaudeConnectorsToggle: View {
    @Environment(NextModel.self) private var model
    let workspace: SettingsWorkspace
    @State private var error: String?

    static let note = "Agents here also get your claude.ai connectors, such as Calendar or Sheets, beside the MCP servers above."

    var body: some View {
        Toggle("Allow claude.ai connectors", isOn: Binding(get: { workspace.claudeConnectors }, set: save))
            .disabled(model.settings.readOnly || model.settings.saving)
            .accessibilityIdentifier("ws.claudeconnectors")
        SaveError(text: error, id: "ws.claudeconnectors.error")
    }

    private func save(_ on: Bool) {
        let op = SettingsOp.workspaceUpdate(workspace: workspace.name, claudeConnectors: on)
        Task { error = await model.settings.save([op]) }
    }
}
