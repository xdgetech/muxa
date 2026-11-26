import MuxaV2
import SwiftUI

/// The live Tasks of the active Task's Workspace as tabs (all live Tasks when
/// the active terminal has no Workspace), and "+" for a new Task there. A
/// tab's close button only hides the tab; the Task closes elsewhere (ADR 0097).
struct FocusTabStrip: View {
    @Environment(NextModel.self) private var model
    @Environment(NextNavigation.self) private var nav
    @Environment(MacChrome.self) private var chrome
    let active: MuxaV2.Terminal?
    @State private var hovered: String?

    private var tasks: [MuxaV2.Terminal] {
        let shown = model.focusTasks.filter { !chrome.hiddenTabs.contains($0.id) || $0.id == active?.id }
        guard let workspace = active?.workspace else { return shown }
        return shown.filter { $0.workspace == workspace }
    }

    var body: some View {
        // Scrolls when the header is short, so the mode switch and Inbox keep their size.
        ScrollView(.horizontal, showsIndicators: false) { strip }
            .frame(minWidth: 0, maxHeight: .infinity, alignment: .leading)
            .layoutPriority(-1)
            .accessibilityElement(children: .contain)
            .accessibilityLabel(active?.workspace.map { "Tasks in \($0)" } ?? "Tasks")
            // A Task opened again, from the sidebar or Go to Task, gets its tab back.
            .onChange(of: active?.id, initial: true) { _, id in if let id { chrome.hiddenTabs.remove(id) } }
    }

    private var strip: some View {
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
    }

    private func tab(_ t: MuxaV2.Terminal, on: Bool) -> some View {
        HStack(spacing: 0) {
            Button { nav.terminal = t.id } label: {
                HStack(spacing: 8) {
                    StateDot(color: model.taskState(t).color, size: 8)
                    Text(t.title).lineLimit(1).fixedSize()
                }
                .padding(.leading, 12)
                .padding(.trailing, 4)
                .frame(maxHeight: .infinity)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityIdentifier("tab.\(t.id)")
            .accessibilityAddTraits(on ? .isSelected : [])
            Button { hide(t, among: tasks) } label: {
                Image(systemName: "xmark")
                    .font(.system(size: 9, weight: .bold))
                    .frame(width: 18, height: 18)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .opacity(on || hovered == t.id ? 1 : 0)
            .help("Close Tab (the Task keeps running)")
            .accessibilityLabel("Close the tab of \(t.title)")
            .accessibilityIdentifier("tab.close.\(t.id)")
            .padding(.trailing, 8)
        }
        .font(.system(size: 13))
        .foregroundStyle(on ? Palette.text : Palette.textSecondary)
        .frame(maxHeight: .infinity)
        .background(on ? Palette.ground : .clear, ignoresSafeAreaEdges: [])
        .overlay(alignment: .bottom) {
            Rectangle().fill(on ? model.taskState(t).color : .clear).frame(height: 2)
        }
        .onHover { hovered = $0 ? t.id : (hovered == t.id ? nil : hovered) }
    }

    /// Hides `t`'s tab. The active tab hands Focus to its neighbour, or to
    /// Home when it was the last, since the active Task always has a tab.
    private func hide(_ t: MuxaV2.Terminal, among shown: [MuxaV2.Terminal]) {
        chrome.hiddenTabs.insert(t.id)
        guard t.id == active?.id, let at = shown.firstIndex(where: { $0.id == t.id }) else { return }
        let rest = shown.filter { $0.id != t.id }
        if rest.isEmpty {
            nav.focus = false
        } else {
            nav.terminal = rest[min(at, rest.count - 1)].id
        }
    }
}
