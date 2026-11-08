#if canImport(UIKit)
import MuxaSession
import SwiftTerm
import UIKit

/// A pane's scrollback, read once from the host, in an emulator as wide as the pane.
/// Touch, trackpad and wheel scroll it; it takes no keys. It opens at the newest line.
@MainActor
public final class HistoryView: UIView {
    private let history: PaneHistory
    private var terminal: ReadOnlyTerminal?
    private let replies = DropReplies()
    private var fittedSize: CGSize = .zero

    private static let maxFontSize: CGFloat = 16

    public init(history: PaneHistory) {
        self.history = history
        super.init(frame: .zero)
        backgroundColor = .black
    }

    required init?(coder: NSCoder) { nil }

    public override var isAccessibilityElement: Bool { get { true } set {} }
    public override var accessibilityLabel: String? { get { "Scrollback" } set {} }
    public override var accessibilityValue: String? {
        get { terminal.map { String(decoding: $0.getBufferAsData(kind: .normal), as: UTF8.self) } }
        set {}
    }

    public override func layoutSubviews() {
        super.layoutSubviews()
        guard bounds.width > 0, bounds.height > 0, bounds.size != fittedSize else { return }
        fittedSize = bounds.size
        let cols = max(history.cols, 1)
        let metrics = CellMetrics(scale: max(window?.screen.scale ?? traitCollection.displayScale, 1))
        let size = min(metrics.largestFontSize(cols: cols, rows: nil, in: bounds.size), Self.maxFontSize)
        let font = UIFont.monospacedSystemFont(ofSize: size, weight: .regular)
        let cell = metrics.cell(for: font)
        let rows = max(1, Int(bounds.height / cell.height))
        // Less than one cell of slack, so SwiftTerm's floor(frame / cell) is exactly the grid.
        let width = cell.width * CGFloat(cols) + min(0.5, cell.width / 2)
        let frame = CGRect(
            x: max(0, ((bounds.width - width) / 2).rounded(.down)), y: 0, width: width, height: bounds.height)
        // A new size feeds the history again into a new emulator.
        terminal?.removeFromSuperview()
        let next = ReadOnlyTerminal(
            frame: frame, font: font,
            options: TerminalOptions(cols: cols, rows: rows, scrollback: history.lineCount + rows))
        next.terminalDelegate = replies
        addSubview(next)
        if next.terminalDimensions.cols != cols || next.terminalDimensions.rows != rows {
            next.resize(cols: cols, rows: rows)
        }
        next.feed(byteArray: [UInt8](history.terminalBytes)[...])
        terminal = next
    }
}

/// The emulator for reading: it scrolls, but never takes the keyboard.
private final class ReadOnlyTerminal: TerminalView {
    override var canBecomeFirstResponder: Bool { false }
}
#endif
