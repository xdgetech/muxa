import SwiftUI

/// An uppercase section label: `WORKSPACES`, `NEEDS YOU · 2`.
struct SectionLabel: View {
    let title: String
    var count: Int?
    var color = Palette.textTertiary

    var body: some View {
        HStack(spacing: 6) {
            Text(title.uppercased()).foregroundStyle(color)
            if let count { Text("\(count)").foregroundStyle(Palette.textTertiary) }
        }
        .font(.system(size: 11, weight: .semibold))
        .tracking(0.66)
    }
}

/// A small rounded label: a kind, an access level, a state or a due date.
struct Badge: View {
    let text: String
    var foreground = Palette.textMid
    var background = Palette.pill

    var body: some View {
        Text(text)
            .font(.system(size: 11.5, weight: .semibold))
            .foregroundStyle(foreground)
            .padding(.horizontal, 8)
            .padding(.vertical, 2)
            .background(background, in: RoundedRectangle(cornerRadius: 5))
    }
}

/// A label over a value on a raised card.
struct FactCard: View {
    let label: String
    let value: String

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(label).font(.system(size: 11.5)).foregroundStyle(Palette.textTertiary)
            Text(value).font(.system(size: 14)).foregroundStyle(Palette.text)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.horizontal, 14)
        .padding(.vertical, 12)
        .background(Palette.cardRaised, in: RoundedRectangle(cornerRadius: 10))
        .overlay(RoundedRectangle(cornerRadius: 10).strokeBorder(Palette.border))
        .accessibilityElement(children: .combine)
    }
}

extension View {
    /// A hand-drawn list row: radius 8, the selected one filled and outlined.
    func selectableRow(_ selected: Bool) -> some View {
        background(selected ? Palette.raised : .clear, in: RoundedRectangle(cornerRadius: 8))
            .overlay(RoundedRectangle(cornerRadius: 8).strokeBorder(selected ? Palette.rowBorder : .clear))
    }

    /// A raised card: radius 10 with a hairline.
    func raisedCard(radius: CGFloat = 10) -> some View {
        background(Palette.cardRaised, in: RoundedRectangle(cornerRadius: radius))
            .overlay(RoundedRectangle(cornerRadius: radius).strokeBorder(Palette.border))
    }

    /// A fixed-width column with a hairline on its trailing edge.
    func column(width: CGFloat, background: Color = Palette.surface) -> some View {
        frame(width: width)
            .frame(maxHeight: .infinity, alignment: .top)
            .background(background)
            .overlay(alignment: .trailing) { Rectangle().fill(Palette.border).frame(width: 1) }
    }
}
