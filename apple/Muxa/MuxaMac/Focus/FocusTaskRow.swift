import MuxaV2
import SwiftUI

/// One terminal in Focus's sidebar: its state dot, its title and the state the
/// program reports. One accessibility element, `terminal.<id>`.
struct FocusTaskRow: View {
    enum Style {
        /// On the amber ground, with the Workspace before the title.
        case needsYou
        /// With the working place under the title.
        case task
        /// One line: Routine runs, shells and terminals muxa did not start.
        case other
    }

    let terminal: MuxaV2.Terminal
    var style = Style.task
    var loop: LoopInfo?
    let selected: Bool
    let open: () -> Void

    var body: some View {
        Button(action: open) {
            HStack(spacing: 8) {
                StateDot(color: terminal.state.color, size: 8)
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
                if let state {
                    Text(state)
                        .font(.system(size: 11))
                        .foregroundStyle(style == .needsYou ? Palette.textSecondary : terminal.state.color)
                }
            }
            .font(.system(size: 13))
            .foregroundStyle(selected || style == .needsYou ? Palette.text : Palette.textMid)
            .padding(.horizontal, 10)
            .padding(.vertical, style == .task ? 4 : 6)
            .background(style == .needsYou ? Palette.bannerBack : .clear, in: RoundedRectangle(cornerRadius: 7))
            .selectableRow(selected)
            .contentShape(Rectangle())
            .accessibilityElement(children: .combine)
        }
        .buttonStyle(.plain)
        .accessibilityIdentifier("terminal.\(terminal.id)")
        .accessibilityAddTraits(selected ? .isSelected : [])
    }

    private var title: String {
        guard style == .needsYou, let ws = terminal.workspace else { return terminal.title }
        return "\(ws) / \(terminal.title)"
    }

    /// The working place, and the loop's attempts while one runs.
    private var detail: String? {
        guard style == .task else { return nil }
        let parts = [terminal.placeText, loop.map(LoopText.badge)].compactMap { $0 }
        return parts.isEmpty ? nil : parts.joined(separator: " · ")
    }

    /// Only what the program reported (OSC 7501); a Needs-you row keeps its
    /// own word, `waiting` or `blocked`.
    private var state: String? {
        guard let reported = terminal.status?.state else { return nil }
        return style == .needsYou ? reported : TaskState(reported).title.lowercased()
    }
}
