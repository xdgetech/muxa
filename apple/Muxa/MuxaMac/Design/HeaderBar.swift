import SwiftUI

/// The desktop mocks' top bar: one row on the surface tone with a hairline
/// under it. It replaces the window toolbar on screens that use it.
struct MacHeaderBar<Leading: View, Trailing: View>: View {
    var height: CGFloat = 52
    /// The main window hides its title bar; this keeps clear of the window buttons.
    var leadingInset: CGFloat = 78
    @ViewBuilder var leading: Leading
    @ViewBuilder var trailing: Trailing

    var body: some View {
        HStack(spacing: 16) {
            leading
            Spacer(minLength: 8)
            trailing
        }
        .padding(.leading, leadingInset)
        .padding(.trailing, 16)
        .frame(height: height)
        .background(Palette.surface)
        .overlay(alignment: .bottom) { Rectangle().fill(Palette.border).frame(height: 1) }
    }
}

/// The square "m" mark the mocks put before the app's name.
struct LogoTile: View {
    var size: CGFloat = 22

    var body: some View {
        Text("m")
            .font(.system(size: size * 0.6, weight: .bold))
            .foregroundStyle(Palette.ground)
            .frame(width: size, height: size)
            .background(Palette.text, in: RoundedRectangle(cornerRadius: size * 0.27))
            .accessibilityHidden(true)
    }
}

/// A 34 pt outlined header button, with an optional key hint.
struct HeaderButton: View {
    let title: String
    var systemImage: String?
    var key: String?
    var tint = Palette.textMid
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            HStack(spacing: 8) {
                if let systemImage { Image(systemName: systemImage).font(.system(size: 12, weight: .semibold)) }
                Text(title)
                if let key { KeyHint(key) }
            }
            .font(.system(size: 13))
            .foregroundStyle(tint)
            .padding(.horizontal, 12)
            .frame(height: 34)
            .overlay(RoundedRectangle(cornerRadius: 8).strokeBorder(Palette.borderStrong))
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .help(key.map { "\(title) (\($0))" } ?? title)
    }
}
