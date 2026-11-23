import AppKit
import MuxaSession
import MuxaV2
import SwiftTerm
import SwiftUI

/// One terminal on protocol v2 in the Mac app: SwiftTerm's AppKit view, which
/// takes keys and input methods itself and hands over bytes. This device is
/// the terminal's last active viewer, so the view's grid is the terminal's
/// size (ADR 0067 decision 7). The iOS app has its own `TerminalPane`.
struct TerminalPane: NSViewRepresentable {
    @Environment(NextModel.self) private var model
    let terminal: MuxaV2.Terminal
    /// This pane has the keyboard; a new `focusRequest` moves it here.
    var active = true
    var focusRequest = 0
    /// The user clicked into this pane.
    var onActivate: () -> Void = {}

    func makeCoordinator() -> Coordinator { Coordinator(model: model, terminal: terminal.id) }

    func makeNSView(context: Context) -> SwiftTerm.TerminalView {
        let settings = TerminalSettings.shared
        let view = FocusReportingTerminalView(frame: .zero, font: MacTerminalLook.font(settings))
        view.onFocus = { context.coordinator.onActivate() }
        view.terminalDelegate = context.coordinator
        MacTerminalLook.colors(settings.theme, on: view)
        context.coordinator.look = MacTerminalLook.key(settings)
        view.setAccessibilityIdentifier("focus.pane")
        context.coordinator.view = view
        return view
    }

    func updateNSView(_ view: SwiftTerm.TerminalView, context: Context) {
        context.coordinator.onActivate = onActivate
        let settings = TerminalSettings.shared
        if context.coordinator.look != MacTerminalLook.key(settings) {
            context.coordinator.look = MacTerminalLook.key(settings)
            view.font = MacTerminalLook.font(settings)
            MacTerminalLook.colors(settings.theme, on: view)
        }
        guard active, context.coordinator.focusRequest != focusRequest else { return }
        context.coordinator.focusRequest = focusRequest
        // The view may not be in its window yet.
        DispatchQueue.main.async { view.window?.makeFirstResponder(view) }
    }

    static func dismantleNSView(_ view: SwiftTerm.TerminalView, coordinator: Coordinator) {
        coordinator.unbind()
    }

    @MainActor
    final class Coordinator: NSObject, TerminalViewDelegate {
        private let model: NextModel
        private let terminal: String
        weak var view: SwiftTerm.TerminalView?
        var onActivate: () -> Void = {}
        /// The last focus request this pane carried out.
        var focusRequest = -1
        /// The terminal settings the view shows.
        var look = ""
        private var channel: UInt32?
        private var grid: (cols: Int, rows: Int)?
        private var attaching: Task<Void, Never>?
        private var ended = false

        init(model: NextModel, terminal: String) {
            self.model = model
            self.terminal = terminal
        }

        func unbind() {
            view = nil
            attaching?.cancel()
            guard let channel else { return }
            self.channel = nil
            let model = model
            Task { await model.detach(channel) }
        }

        /// Attaches at the view's grid. A new grid attaches again, so the
        /// server resizes the terminal and paints it at the new size. Waits
        /// for a window resize to settle.
        private func attach() {
            guard !ended else { return }
            attaching?.cancel()
            attaching = Task { [weak self] in
                guard let self else { return }
                try? await Task.sleep(for: .milliseconds(200))
                guard !Task.isCancelled, let grid else { return }
                if let old = channel {
                    channel = nil
                    await model.detach(old)
                }
                channel = await model.attach(
                    terminal, cols: UInt16(grid.cols), rows: UInt16(grid.rows)
                ) { [weak self] event in self?.handle(event) }
                if channel != nil, view == nil { unbind() }
            }
        }

        private func handle(_ event: V2Event) {
            guard let view else { return }
            switch event {
            case .paint:
                // RIS: the next output draws a fresh screen.
                view.feed(byteArray: [0x1b, 0x63][...])
            case .output(_, let bytes):
                view.feed(byteArray: bytes[...])
            case .closed(let ch, let exit):
                guard ch == channel else { return }
                channel = nil
                if let exit {
                    // The program ended; show why and stay on the last screen.
                    ended = true
                    view.feed(text: "\r\n[\(exit.reason)]\r\n")
                } else {
                    // Not resumed after a reconnect: attach again, with a fresh screen.
                    attach()
                }
            default:
                break
            }
        }

        // MARK: TerminalViewDelegate

        func sizeChanged(source: SwiftTerm.TerminalView, newCols: Int, newRows: Int) {
            guard newCols > 0, newRows > 0, grid?.cols != newCols || grid?.rows != newRows else { return }
            grid = (newCols, newRows)
            attach()
        }

        func send(source: SwiftTerm.TerminalView, data: ArraySlice<UInt8>) {
            guard let channel, !data.isEmpty else { return }
            let bytes = Array(data)
            Task { await model.input(channel, bytes) }
        }

        func setTerminalTitle(source: SwiftTerm.TerminalView, title: String) {}
        func hostCurrentDirectoryUpdate(source: SwiftTerm.TerminalView, directory: String?) {}
        func scrolled(source: SwiftTerm.TerminalView, position: Double) {}
        func clipboardCopy(source: SwiftTerm.TerminalView, content: Data) {
            NSPasteboard.general.clearContents()
            NSPasteboard.general.setData(content, forType: .string)
        }
        func rangeChanged(source: SwiftTerm.TerminalView, startY: Int, endY: Int) {}
    }
}

/// The device's terminal settings on SwiftTerm's AppKit view.
@MainActor
enum MacTerminalLook {
    /// What the view shows; a change re-applies the font and colors.
    static func key(_ s: TerminalSettings) -> String { "\(s.themeID)|\(s.fontName)|\(s.fontSize)" }

    static func font(_ s: TerminalSettings) -> NSFont {
        s.font(size: s.fontSize).map { $0 as NSFont } ?? .monospacedSystemFont(ofSize: CGFloat(s.fontSize), weight: .regular)
    }

    static func colors(_ theme: TerminalTheme, on view: SwiftTerm.TerminalView) {
        view.nativeBackgroundColor = NSColor(theme.background)
        view.nativeForegroundColor = NSColor(theme.foreground)
        view.installColors(theme.ansi.map { $0.map(color) } ?? SwiftTerm.Color.defaultInstalledColors)
        if let cursor = theme.cursor { view.caretColor = NSColor(cursor) }
        if let selection = theme.selection { view.selectedTextBackgroundColor = NSColor(selection) }
    }

    private static func color(_ c: RGB) -> SwiftTerm.Color {
        SwiftTerm.Color(red: UInt16(c.r) * 257, green: UInt16(c.g) * 257, blue: UInt16(c.b) * 257)
    }
}

extension NSColor {
    convenience init(_ c: RGB) {
        self.init(srgbRed: CGFloat(c.r) / 255, green: CGFloat(c.g) / 255, blue: CGFloat(c.b) / 255, alpha: 1)
    }
}

/// Tells which pane the user clicked into, so the list's pick goes to that pane.
final class FocusReportingTerminalView: SwiftTerm.TerminalView {
    var onFocus: () -> Void = {}

    override func mouseDown(with event: NSEvent) {
        onFocus()
        super.mouseDown(with: event)
    }
}
