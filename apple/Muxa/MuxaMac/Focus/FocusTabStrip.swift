import MuxaV2
import SwiftUI

/// The live Tasks of the active Task's Workspace as tabs (all live Tasks when
/// the active terminal has no Workspace), and "+" for a new Task there.
struct FocusTabStrip: View {
    @Environment(NextModel.self) private var model
    @Environment(NextNavigation.self) private var nav
    let active: MuxaV2.Terminal?

    private var tasks: [MuxaV2.Terminal] {
        guard let workspace = active?.workspace else { return model.liveTasks }
        return model.liveTasks.filter { $0.workspace == workspace }
    }

    var body: some View {
        HStack(spacing: 2) {
            ForEach(tasks) { t in tab(t, on: t.id == active?.id) }
            NewTaskButton(id: "focus.newtask", workspace: active?.workspace)
                .labelStyle(.iconOnly)
                .buttonStyle(.plain)
                .font(.system(size: 15, weight: .medium))
                .foregroundStyle(Palette.textSecondary)
                .padding(.horizontal, 10)
                .frame(maxHeight: .infinity)
                .help("New Task (\(Shortcut.newTask.display))")
        }
        .frame(maxHeight: .infinity, alignment: .leading)
        .clipped()
        .accessibilityElement(children: .contain)
        .accessibilityLabel(active?.workspace.map { "Tasks in \($0)" } ?? "Tasks")
    }

    private func tab(_ t: MuxaV2.Terminal, on: Bool) -> some View {
        Button { nav.terminal = t.id } label: {
            HStack(spacing: 8) {
                StateDot(color: t.state.color, size: 8)
                Text(t.title).lineLimit(1).fixedSize()
            }
            .font(.system(size: 13))
            .foregroundStyle(on ? Palette.text : Palette.textSecondary)
            .padding(.horizontal, 12)
            .frame(maxHeight: .infinity)
            .background(on ? Palette.ground : .clear, ignoresSafeAreaEdges: [])
            .overlay(alignment: .bottom) {
                Rectangle().fill(on ? t.state.color : .clear).frame(height: 2)
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityIdentifier("tab.\(t.id)")
        .accessibilityAddTraits(on ? .isSelected : [])
    }
}
