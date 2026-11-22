import MuxaV2
import SwiftUI

/// The Routines tab: the Workspace's Routines gate, then the Routines of
/// `.muxa/routines.toml` with their schedule. The mock's "Goes to" column is
/// left out: a Routine's spec does not say where its output goes.
struct MacRoutinesTable: View {
    @Environment(NextModel.self) private var model
    let workspace: SettingsWorkspace
    @State private var editing: RoutineDraft?
    @State private var error: String?

    private enum Width {
        static let name: CGFloat = 240
        static let edit: CGFloat = 60
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            gate
            SaveError(text: error, id: "settings.error")
            if let problem = workspace.routinesProblem {
                Text("routines.toml cannot be used as it is: \(problem)")
                    .foregroundStyle(Theme.failure)
                    .fixedSize(horizontal: false, vertical: true)
            }
            VStack(spacing: 0) {
                MacTableHeader(columns: [("Routine", Width.name), ("Schedule", nil), ("", Width.edit)])
                ForEach(workspace.routines) { row($0) }
            }
            MacAddButton(title: "+ Add Routine", id: "routine.add") { editing = RoutineDraft() }
        }
        .sheet(item: $editing) { RoutineEditor(workspace: workspace.name, draft: $0) }
    }

    private var gate: some View {
        HStack(spacing: 14) {
            VStack(alignment: .leading, spacing: 3) {
                Text("Run Routines in this Workspace").font(.system(size: 14, weight: .semibold))
                Text("From .muxa/routines.toml. Off: none of them run, and they show as Turned off.")
                    .font(.system(size: 12))
                    .foregroundStyle(Palette.textTertiary)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            Toggle("Run Routines in this Workspace", isOn: Binding(
                get: { workspace.routinesEnabled },
                set: { on in
                    let op = SettingsOp.routinesGate(workspace: workspace.name, enabled: on, directWrites: workspace.directWrites)
                    Task { error = await model.settings.save([op]) }
                }))
                .labelsHidden()
                .toggleStyle(.switch)
                .accessibilityIdentifier("routines.gate")
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 12)
        .raisedCard()
    }

    private func row(_ r: SettingsRoutine) -> some View {
        let when = WorkspaceRoutinesForm.when(r.spec)
        return Button { editing = RoutineDraft(r) } label: {
            MacTableRow {
                HStack(spacing: 8) {
                    Text(r.name).fontWeight(.medium).lineLimit(1)
                    if !r.spec.enabled { Badge(text: "off") }
                }
                .cell(Width.name)
                Text(when).foregroundStyle(Palette.textMid).lineLimit(1).cell(nil)
                MacEditChip().cell(Width.edit)
            }
        }
        .buttonStyle(.plain)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("\(r.name), \(when)" + (r.spec.enabled ? "" : ", off"))
        .accessibilityAddTraits(.isButton)
        .accessibilityIdentifier("settings.routine.\(r.name)")
    }
}
