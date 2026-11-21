import AppKit
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

    func makeCoordinator() -> Coordinator { Coordinator(model: model, terminal: terminal.id) }

    func makeNSView(context: Context) -> SwiftTerm.TerminalView {
        let view = SwiftTerm.TerminalView(frame: .zero, font: .monospacedSystemFont(ofSize: 13, weight: .regular))
        view.terminalDelegate = context.coordinator
        view.setAccessibilityIdentifier("focus.pane")
        context.coordinator.view = view
        return view
    }

    func updateNSView(_ view: SwiftTerm.TerminalView, context: Context) {}

    static func dismantleNSView(_ view: SwiftTerm.TerminalView, coordinator: Coordinator) {
        coordinator.unbind()
    }

    @MainActor
    final class Coordinator: NSObject, TerminalViewDelegate {
        private let model: NextModel
        private let terminal: String
        weak var view: SwiftTerm.TerminalView?
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
