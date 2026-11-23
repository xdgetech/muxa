import AppKit
import SwiftUI

/// The desktop mocks' top bar: one row on the surface tone with a hairline
/// under it. It replaces the window toolbar on screens that use it.
struct MacHeaderBar<Leading: View, Trailing: View>: View {
    var height: CGFloat = 52
    @ViewBuilder var leading: Leading
    @ViewBuilder var trailing: Trailing
    @State private var leadingInset = WindowButtonsInset.edge

    var body: some View {
        HStack(spacing: 16) {
            leading
            Spacer(minLength: 8)
            trailing
        }
        .padding(.leading, leadingInset)
        .padding(.trailing, WindowButtonsInset.edge)
        .frame(height: height)
        .background(Palette.surface)
        .background(WindowButtonsInset { leadingInset = $0 })
        .overlay(alignment: .bottom) { Rectangle().fill(Palette.border).frame(height: 1) }
    }
}

/// How far a header starts: just right of the window buttons when they sit in
/// its rows, else at the edge. A fixed 78 pt left that much empty in full
/// screen, where the buttons leave the window, and under a visible title bar.
private struct WindowButtonsInset: NSViewRepresentable {
    static let edge: CGFloat = 16
    /// Between the zoom button and the header's first item.
    static let gap: CGFloat = 10
    let changed: (CGFloat) -> Void

    func makeNSView(context: Context) -> Probe {
        let view = Probe()
        view.changed = changed
        return view
    }

    func updateNSView(_ view: Probe, context: Context) {
        view.changed = changed
    }

    final class Probe: NSView {
        var changed: (CGFloat) -> Void = { _ in }
        private var observers: [NSObjectProtocol] = []
        private var reported: CGFloat?

        override func viewDidMoveToWindow() {
            super.viewDidMoveToWindow()
            observers.forEach(NotificationCenter.default.removeObserver)
            observers = []
            guard let window else { return }
            let names: [Notification.Name] = [NSWindow.didEnterFullScreenNotification, NSWindow.didExitFullScreenNotification]
            observers = names.map { name in
                NotificationCenter.default.addObserver(forName: name, object: window, queue: .main) { [weak self] _ in
                    MainActor.assumeIsolated { self?.report() }
                }
            }
            reportLater()
        }

        override func layout() {
            super.layout()
            reportLater()
        }

        /// SwiftUI must not see its state change while it lays this view out.
        private func reportLater() {
            DispatchQueue.main.async { [weak self] in self?.report() }
        }

        private func report() {
            let inset = Self.inset(of: self)
            guard inset != reported else { return }
            reported = inset
            changed(inset)
        }

        static func inset(of header: NSView) -> CGFloat {
            guard let window = header.window, !window.styleMask.contains(.fullScreen),
                  let zoom = window.standardWindowButton(.zoomButton), !zoom.isHidden,
                  let button = zoom.superview?.convert(zoom.frame, to: nil) else { return edge }
            let rows = header.convert(header.bounds, to: nil)
            guard rows.minY < button.maxY, button.minY < rows.maxY else { return edge }
            return button.maxX + gap
        }
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
