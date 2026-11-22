import SwiftUI

/// A shortcut as the mocks print it: mono, small, dim.
struct KeyHint: View {
    let key: String
    var color = Palette.textTertiary

    init(_ key: String, color: Color = Palette.textTertiary) {
        self.key = key
        self.color = color
    }

    var body: some View {
        Text(key)
            .font(.system(size: 11, design: .monospaced))
            .foregroundStyle(color)
            .accessibilityHidden(true)
    }
}

/// The strip of keys under a pane or a panel: `⌘P go to Task  ⌘\ split …`.
struct KeyLegend: View {
    let items: [(key: String, label: String)]

    var body: some View {
        HStack(spacing: 16) {
            ForEach(items, id: \.key) { item in
                HStack(spacing: 5) {
                    KeyHint(item.key, color: Palette.text)
                    Text(item.label)
                }
            }
            Spacer(minLength: 0)
        }
        .font(.system(size: 12))
        .foregroundStyle(Palette.textSecondary)
        .padding(.horizontal, 16)
        .frame(height: 30)
        .background(Palette.pane)
        .overlay(alignment: .top) { Rectangle().fill(Palette.border).frame(height: 1) }
    }
}
