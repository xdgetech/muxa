import MuxaV2
import SwiftUI

/// An Inbox row as the desktop mock: the grip, the place in the user's
/// order, the kind, a one-line title over where it came from, and the due pill.
struct HomeInboxRow: View {
    let item: InboxItem
    /// From 1; nil for a Done item.
    let position: Int?
    let selected: Bool
    /// False when a filter or Done makes the order partial.
    var draggable = true

    var body: some View {
        HStack(alignment: .center, spacing: 6) {
            Text("⋮⋮")
                .font(.system(size: 13))
                .tracking(-2)
                .foregroundStyle(draggable ? Palette.textDim : .clear)
                .frame(width: 14)
                .accessibilityHidden(true)
            Text(position.map(String.init) ?? "")
                .font(.system(size: 12))
                .foregroundStyle(Palette.textTertiary)
                .frame(width: 18, alignment: .trailing)
                .accessibilityHidden(true)
            HStack(alignment: .top, spacing: 10) {
                KindChip(kind: item.kind).padding(.top, 1)
                VStack(alignment: .leading, spacing: 2) {
                    Text(item.title)
                        .font(.system(size: 13.5))
                        .foregroundStyle(position == nil ? Palette.textSecondary : Palette.text)
                        .lineLimit(1)
                    Text(item.origin)
                        .font(.system(size: 12))
                        .foregroundStyle(Palette.textTertiary)
                        .lineLimit(1)
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                if let due = item.due { DuePill(due: due).padding(.top, 1) }
            }
            .padding(.vertical, 7)
        }
        .padding(.leading, 6)
        .padding(.trailing, 8)
        .selectableRow(selected)
        .contentShape(Rectangle())
        .accessibilityElement(children: .combine)
        .accessibilityAddTraits(selected ? [.isButton, .isSelected] : .isButton)
        .accessibilityIdentifier("item.\(item.id)")
    }
}

/// A due date as the mock's pill: red once it has passed.
struct DuePill: View {
    let due: String

    var body: some View {
        let today = NextModel.today()
        let overdue = due < today
        Text(DueLabel.short(due, today: today))
            .font(.system(size: 11.5))
            .foregroundStyle(overdue ? Palette.late : Palette.textMid)
            .padding(.horizontal, 7)
            .padding(.vertical, 2)
            .background(overdue ? ItemKind.failed.chipColors.back : Palette.pill, in: RoundedRectangle(cornerRadius: 5))
            .accessibilityLabel(overdue ? "Overdue \(due)" : "Due \(due)")
    }
}
