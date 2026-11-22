import SwiftUI

/// The mocks' action buttons: a near-white primary, an outlined secondary
/// and a red destructive, 36 pt (30 in panels), with a key chip inside.
struct ActionButtonStyle: ButtonStyle {
    enum Kind { case primary, secondary, destructive }
    var kind = Kind.secondary
    var compact = false

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.system(size: compact ? 12.5 : 13, weight: kind == .secondary ? .medium : .semibold))
            .foregroundStyle(foreground)
            .padding(.horizontal, 12)
            .frame(height: compact ? 30 : 36)
            .background(background, in: RoundedRectangle(cornerRadius: 8))
            .overlay {
                if kind == .secondary {
                    RoundedRectangle(cornerRadius: 8).strokeBorder(Palette.buttonBorder)
                }
            }
            .opacity(configuration.isPressed ? 0.8 : 1)
            .contentShape(Rectangle())
    }

    private var foreground: Color {
        switch kind {
        case .primary: Palette.ground
        case .secondary: Palette.text
        case .destructive: .white
        }
    }

    private var background: Color {
        switch kind {
        case .primary: Palette.text
        case .secondary: Palette.rowSelected
        case .destructive: Palette.destructive
        }
    }
}

/// A button label with its key chip: `Approve ⌘⏎`.
struct ActionLabel: View {
    let title: String
    var key: String?
    var primary = false

    var body: some View {
        HStack(spacing: 8) {
            Text(title)
            if let key {
                Text(key)
                    .font(.system(size: 11, design: .monospaced))
                    .padding(.horizontal, 5)
                    .background(primary ? Color.black.opacity(0.12) : Palette.ground,
                                in: RoundedRectangle(cornerRadius: 4))
                    .foregroundStyle(primary ? Palette.ground : Palette.textSecondary)
            }
        }
    }
}

/// The bar of actions at the foot of a detail pane.
struct ActionBar<Content: View>: View {
    @ViewBuilder var content: Content

    var body: some View {
        HStack(spacing: 8) {
            content
            Spacer(minLength: 0)
        }
        .padding(.horizontal, 28)
        .padding(.vertical, 14)
        .overlay(alignment: .top) { Rectangle().fill(Palette.border).frame(height: 1) }
    }
}
