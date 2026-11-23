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

/// The app icon's mark before the app's name: the prompt glyph of
/// `apple/Muxa/Art/app-icon.swift` on its gradient, at any size.
struct LogoTile: View {
    var size: CGFloat = 22

    var body: some View {
        PromptGlyph()
            .fill(.white)
            .frame(width: size, height: size)
            .background(
                LinearGradient(colors: [Color(hex: 0x8683FF), Color(hex: 0x5E5CE6), Color(hex: 0x3F37C2)],
                               startPoint: .topLeading, endPoint: .bottomTrailing),
                in: RoundedRectangle(cornerRadius: size * 0.225, style: .continuous))
            .accessibilityHidden(true)
    }
}

/// The icon's chevron and cursor bar on its 1024 canvas, scaled to the rect.
/// The numbers are app-icon.swift's; keep the two alike.
private struct PromptGlyph: Shape {
    private static let canvas = 1024.0
    private static let stroke = 82.0

    func path(in rect: CGRect) -> Path {
        let line = CGMutablePath()
        line.addLines(between: [CGPoint(x: 0, y: 0), CGPoint(x: 190, y: 150), CGPoint(x: 0, y: 300)])
        let chevron = line.copy(strokingWithWidth: Self.stroke, lineCap: .round, lineJoin: .round, miterLimit: 10)
        // y runs down here: the bar sits under the chevron's lower end.
        let bar = CGRect(x: 262, y: 300 - Self.stroke / 2, width: 210, height: Self.stroke)
        let cursor = CGPath(roundedRect: bar, cornerWidth: Self.stroke / 2, cornerHeight: Self.stroke / 2, transform: nil)
        let bounds = chevron.boundingBox.union(bar)
        // As the icon: centred, a little right of true centre for the chevron's point.
        let scale = min(rect.width, rect.height) / Self.canvas
        let move = CGAffineTransform(translationX: (Self.canvas - bounds.width) / 2 - bounds.minX + 8,
                                     y: (Self.canvas - bounds.height) / 2 - bounds.minY)
            .concatenating(CGAffineTransform(scaleX: scale, y: scale))
            .concatenating(CGAffineTransform(translationX: rect.minX, y: rect.minY))
        var path = Path(chevron)
        path.addPath(Path(cursor))
        return path.applying(move)
    }
}

/// The start of every main-window header: the mark, the app's name and the
/// host menu, so Home and Focus begin alike.
struct HeaderBrand: View {
    var body: some View {
        HStack(spacing: 8) {
            LogoTile()
            Text("muxa").font(.system(size: 13, weight: .semibold))
            HostMenu()
                .menuStyle(.button)
                .buttonStyle(.plain)
                .menuIndicator(.hidden)
                .fixedSize()
        }
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
