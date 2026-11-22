import MuxaSession
import MuxaUI
import MuxaV2
import SwiftUI
import UIKit

/// One terminal on protocol v2: the screen at the grid this view fits, and keys
/// sent as bytes. This device is the terminal's last active viewer, so the grid
/// is the terminal's size (ADR 0067 decision 7).
final class V2PaneView: UIView {
    let screen = PaneTerminalView()
    let input = KeyInputView()
    /// The grid that fits this view at the default font changed.
    var onGrid: (PaneViewport) -> Void = { _ in }
    private var grid: PaneViewport?

    override init(frame: CGRect) {
        super.init(frame: frame)
        backgroundColor = PaneTerminalView.background
        addSubview(screen)
        addSubview(input)
        input.screenText = { [weak self] in self?.screen.screenText ?? "" }
        input.caretFrame = { [weak self] in self?.screen.cursorFrame ?? .zero }
        addGestureRecognizer(UITapGestureRecognizer(target: self, action: #selector(tapped)))
        input.accessibilityIdentifier = "focus.pane"
    }

    required init?(coder: NSCoder) { nil }

    @objc private func tapped() { input.becomeFirstResponder() }

    override func layoutSubviews() {
        super.layoutSubviews()
        screen.frame = bounds
        input.frame = bounds
        let font = PaneViewport.fontSize(compact: traitCollection.horizontalSizeClass == .compact)
        guard let fit = screen.viewportGrid(in: bounds.size, fontSize: font), fit != grid else { return }
        grid = fit
        onGrid(fit)
    }
}

struct TerminalPane: UIViewRepresentable {
    @Environment(NextModel.self) private var model
    let terminal: Terminal

    func makeCoordinator() -> Coordinator { Coordinator(model: model, terminal: terminal.id) }

    func makeUIView(context: Context) -> V2PaneView {
        let view = V2PaneView()
        context.coordinator.bind(view)
        return view
    }

    func updateUIView(_ view: V2PaneView, context: Context) {}

    static func dismantleUIView(_ view: V2PaneView, coordinator: Coordinator) {
        coordinator.unbind()
    }

    @MainActor
    final class Coordinator {
        private let model: NextModel
        private let terminal: String
        private weak var view: V2PaneView?
        private var channel: UInt32?
        private var grid: PaneViewport?
        private var attaching = false
        private var ended = false

        init(model: NextModel, terminal: String) {
            self.model = model
            self.terminal = terminal
        }

        func bind(_ view: V2PaneView) {
            self.view = view
            view.onGrid = { [weak self] grid in self?.gridChanged(grid) }
            view.input.onOutput = { [weak self] output in self?.send(output) }
            view.input.onPaste = { [weak self] text in self?.send(Array(text.utf8)) }
        }

        func unbind() {
            view = nil
            guard let channel else { return }
            self.channel = nil
            let model = model
            Task { await model.detach(channel) }
        }

        private func gridChanged(_ grid: PaneViewport) {
            self.grid = grid
            if let channel {
                Task { await model.resize(channel, cols: UInt16(grid.cols), rows: UInt16(grid.rows)) }
            } else {
                attach()
            }
        }

        private func attach() {
            guard !attaching, !ended, let grid else { return }
            attaching = true
            Task {
                channel = await model.attach(
                    terminal, cols: UInt16(grid.cols), rows: UInt16(grid.rows)
                ) { [weak self] event in self?.handle(event) }
                attaching = false
                if channel != nil, view == nil { unbind() }
            }
        }

        private func handle(_ event: V2Event) {
            guard let view, let grid else { return }
            switch event {
            case .paint:
                view.screen.reset(cols: grid.cols, rows: grid.rows, data: Data())
            case .output(_, let bytes):
                view.screen.feed(Data(bytes))
            case .closed(_, let exit):
                channel = nil
                if let exit {
                    // The program ended; show why and stay on the last screen.
                    ended = true
                    view.screen.feed(Data("\r\n[\(exit.reason)]\r\n".utf8))
                } else {
                    // Not resumed after a reconnect: attach again, with a fresh screen.
                    attach()
                }
            default:
                break
            }
        }

        private func send(_ output: KeyMap.Output) {
            switch output {
            case .text(let text): send(Array(text.utf8))
            case .keys(let names): send(names.compactMap(KeyBytes.bytes).flatMap { $0 })
            case .none: break
            }
        }

        private func send(_ bytes: [UInt8]) {
            guard let channel, !bytes.isEmpty else { return }
            Task { await model.input(channel, bytes) }
        }
    }
}
