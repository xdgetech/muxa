import AppKit
import MuxaV2
import SwiftUI

/// The active Task's Reader over the whole window, from the right. Its left
/// edge drags it wider or narrower, the button fills the window, and Esc
/// closes it. The width is kept for the next time. It takes the keyboard from
/// the terminal while open and gives it back when it closes. Esc closes it
/// wherever the keyboard is: a click on the panel leaves it with the window.
struct ReaderOverlay: View {
    @Environment(NextNavigation.self) private var nav
    let terminal: MuxaV2.Terminal
    /// Of the window's width.
    @AppStorage("reader.width") private var fraction = Self.defaultFraction
    @State private var dragStart: Double?
    @FocusState private var focused: Bool

    private static let defaultFraction = 0.55
    private static let range = 0.3...1.0

    var body: some View {
        GeometryReader { geo in
            HStack(spacing: 0) {
                Spacer(minLength: 0)
                    .contentShape(Rectangle())
                    .onTapGesture { nav.showReader = false }
                panel
                    .frame(width: geo.size.width * fraction)
                    .overlay(alignment: .leading) { handle(width: geo.size.width) }
            }
        }
        .background(Color.black.opacity(fraction < 1 ? 0.25 : 0).allowsHitTesting(false))
        .background(EscapeKey { nav.showReader = false })
        .onAppear(perform: takeKeyboard)
        .onDisappear { nav.refocus() }
    }

    /// The terminal, an AppKit view, keeps the keyboard until it lets go; the
    /// panel takes it a moment later, as Go to Task does.
    private func takeKeyboard() {
        NSApp.keyWindow?.makeFirstResponder(nil)
        DispatchQueue.main.async { focused = true }
    }

    private var panel: some View {
        VStack(spacing: 0) {
            HStack(spacing: 8) {
                Text("Reader").font(.system(size: 15, weight: .semibold)).accessibilityAddTraits(.isHeader)
                Text(terminal.title).font(.system(size: 12)).foregroundStyle(Palette.textTertiary).lineLimit(1)
                Spacer(minLength: 8)
                Button(fraction < 1 ? "Fill Window" : "Restore Size",
                       systemImage: fraction < 1 ? "arrow.up.left.and.arrow.down.right" : "arrow.down.right.and.arrow.up.left") {
                    fraction = fraction < 1 ? 1 : Self.defaultFraction
                }
                .labelStyle(.iconOnly)
                .buttonStyle(.plain)
                .foregroundStyle(Palette.textSecondary)
                .accessibilityIdentifier("reader.size")
                Button { nav.showReader = false } label: {
                    Text("esc")
                        .font(.system(size: 11, weight: .medium, design: .monospaced))
                        .foregroundStyle(Palette.textSecondary)
                        .padding(.horizontal, 8)
                        .frame(height: 28)
                        .overlay(RoundedRectangle(cornerRadius: 6).strokeBorder(Palette.borderStrong))
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .accessibilityLabel("Close Reader")
                .accessibilityIdentifier("panel.close")
            }
            .padding(.horizontal, 14)
            .padding(.vertical, 10)
            ReaderPanel(terminal: terminal).id(terminal.id)
        }
        .frame(maxHeight: .infinity)
        .focusable()
        .focusEffectDisabled()
        .focused($focused)
        .background(Palette.surface)
        .overlay(alignment: .leading) { Rectangle().fill(Palette.border).frame(width: 1) }
        .shadow(color: .black.opacity(0.35), radius: 16, x: -4)
    }

    /// The left edge: drag to resize.
    private func handle(width: CGFloat) -> some View {
        Rectangle()
            .fill(Color.clear)
            .frame(width: 8)
            .contentShape(Rectangle())
            .onHover { inside in
                if inside { NSCursor.resizeLeftRight.push() } else { NSCursor.pop() }
            }
            .gesture(
                DragGesture(minimumDistance: 1, coordinateSpace: .global)
                    .onChanged { drag in
                        let start = dragStart ?? fraction
                        dragStart = start
                        let next = start - Double(drag.translation.width / max(width, 1))
                        fraction = min(max(next, Self.range.lowerBound), Self.range.upperBound)
                    }
                    .onEnded { _ in dragStart = nil }
            )
            .accessibilityHidden(true)
    }
}

/// Runs `pressed` on Esc in this view's window, whichever view there has the
/// keyboard, and keeps the key from it. A sheet or Go to Task over the window
/// keeps its own Esc.
private struct EscapeKey: NSViewRepresentable {
    @Environment(NextNavigation.self) private var nav
    let pressed: () -> Void

    func makeNSView(context: Context) -> NSView {
        let view = NSView()
        context.coordinator.view = view
        context.coordinator.install()
        return view
    }

    func updateNSView(_ view: NSView, context: Context) {
        context.coordinator.pressed = pressed
        context.coordinator.nav = nav
    }

    static func dismantleNSView(_ view: NSView, coordinator: Coordinator) {
        coordinator.remove()
    }

    func makeCoordinator() -> Coordinator { Coordinator(nav: nav, pressed: pressed) }

    @MainActor
    final class Coordinator {
        weak var view: NSView?
        var nav: NextNavigation
        var pressed: () -> Void
        private var monitor: Any?
        private static let escape: UInt16 = 53

        init(nav: NextNavigation, pressed: @escaping () -> Void) {
            self.nav = nav
            self.pressed = pressed
        }

        func install() {
            monitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { [weak self] event in
                MainActor.assumeIsolated { self?.handle(event) ?? false } ? nil : event
            }
        }

        func remove() {
            if let monitor { NSEvent.removeMonitor(monitor) }
            monitor = nil
        }

        private func handle(_ event: NSEvent) -> Bool {
            let modifiers = event.modifierFlags.intersection(.deviceIndependentFlagsMask).subtracting([.capsLock, .numericPad, .function])
            guard event.keyCode == Self.escape, modifiers.isEmpty, let window = view?.window, event.window === window,
                  window.attachedSheet == nil, !nav.showPalette else { return false }
            pressed()
            return true
        }
    }
}
