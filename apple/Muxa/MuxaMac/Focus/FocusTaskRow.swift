import MuxaV2
import SwiftUI

/// One terminal in Focus's sidebar: its state dot, its title and the state the
/// program reports. One accessibility element, `terminal.<id>`. A Task's row
/// closes the Task, after asking, from its hover × or its context menu, and
/// pauses or resumes it from the menu. A paused Task's row is dimmed.
struct FocusTaskRow: View {
    @Environment(NextModel.self) private var model
    @Environment(MacChrome.self) private var chrome
    enum Style {
        /// On the amber ground, with the Workspace before the title.
        case needsYou
        /// With the working place under the title.
        case task
        /// One line under a Workspace's "No repo": the header says the place.
        case loose
        /// One line: Routine runs, shells and terminals muxa did not start.
        case other
    }

    let terminal: MuxaV2.Terminal
    var style = Style.task
    var loop: LoopInfo?
    /// Under a Workspace's repo or its "No repo" line.
    var indent: CGFloat = 0
    let selected: Bool
    let open: () -> Void
    @State private var hovered = false

    var body: some View {
        row
            .overlay(alignment: .trailing) {
                if closable {
                    Button { chrome.closingTask = terminal } label: {
                        Image(systemName: "xmark")
                            .font(.system(size: 9, weight: .bold))
                            .foregroundStyle(Palette.textSecondary)
                            .frame(width: 18, height: 18)
                            .background(Palette.surface, in: RoundedRectangle(cornerRadius: 4))
                            .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                    .padding(.trailing, 6)
                    .help("Close Task…")
                    .accessibilityLabel("Close \(terminal.title)")
                    .accessibilityIdentifier("row.close.\(terminal.id)")
                }
            }
            .onHover { hovered = $0 }
            .contextMenu {
                if terminal.isTaskAgent {
                    PauseMenuItems(terminal: terminal)
                    Button("Close Task…") { chrome.closingTask = terminal }
                }
            }
    }

    private var row: some View {
        Button(action: open) {
            HStack(spacing: 8) {
                StateDot(color: taskState.color, size: 8)
                VStack(alignment: .leading, spacing: 1) {
                    Text(title).lineLimit(1).truncationMode(.tail)
                    if let detail {
                        Text(detail)
                            .font(.system(size: 11))
                            .foregroundStyle(loop?.reason == "limit" ? Theme.attention : Palette.textTertiary)
                            .lineLimit(1)
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                // The close button takes the state's place while the pointer is on the row.
                if let state, !closable {
                    Text(state)
                        .font(.system(size: 11))
                        .foregroundStyle(style == .needsYou ? Palette.textSecondary : taskState.color)
                }
            }
            .font(.system(size: 13))
            .foregroundStyle(selected || style == .needsYou ? Palette.text : Palette.textMid)
            .padding(.leading, 10 + indent)
            .padding(.trailing, 10)
            .padding(.vertical, style == .task || style == .loose ? 4 : 6)
            .background(style == .needsYou ? Palette.bannerBack : .clear, in: RoundedRectangle(cornerRadius: 7))
            .selectableRow(selected)
            .opacity(taskState == .paused ? 0.6 : 1)
            .contentShape(Rectangle())
            .accessibilityElement(children: .combine)
        }
        .buttonStyle(.plain)
        .accessibilityIdentifier("terminal.\(terminal.id)")
        .accessibilityAddTraits(selected ? .isSelected : [])
    }

    private var closable: Bool { terminal.isTaskAgent && hovered }

    private var taskState: TaskState { model.taskState(terminal) }

    private var title: String {
        guard style == .needsYou, let ws = terminal.workspace else { return terminal.title }
        return "\(ws) / \(terminal.title)"
    }

    /// The working place, and the loop's attempts while one runs.
    private var detail: String? {
        guard style == .task || style == .loose else { return nil }
        let place = style == .task ? terminal.placeText : nil
        let parts = [place, loop.map(LoopText.badge)].compactMap { $0 }
        return parts.isEmpty ? nil : parts.joined(separator: " · ")
    }

    /// Only what the program reported (OSC 7501), or `paused` (ADR 0104); a
    /// Needs-you row keeps its own word, `waiting` or `blocked`.
    private var state: String? {
        if taskState == .paused { return TaskState.paused.title.lowercased() }
        guard let reported = terminal.status?.state else { return nil }
        return style == .needsYou ? reported : TaskState(reported).title.lowercased()
    }
}
