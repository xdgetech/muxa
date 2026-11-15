#if canImport(UIKit)
import MuxaSession
import SwiftTerm
import UIKit

/// A pane's scrollback, read once from the host, in an emulator as wide as the pane.
/// Touch, trackpad and wheel scroll it; it takes no keys. It opens at the newest line.
@MainActor
public final class HistoryView: UIView {
    /// The user pulled up past the newest line: back to the live pane.
    public var onLive: () -> Void = {}
    private let history: PaneHistory
    private var terminal: ReadOnlyTerminal?
    private let replies = DropReplies()
    private var fittedSize: CGSize = .zero
    /// Points above the newest line, kept when a new size feeds a new emulator.
    private var distance: CGFloat = 0
    private var lineHeight: CGFloat = 1

    /// After each scroll: whether the newest line is at the bottom.
    public var onScroll: (_ atNewest: Bool) -> Void = { _ in }

    private static let maxFontSize: CGFloat = 16

    public init(history: PaneHistory) {
        self.history = history
        super.init(frame: .zero)
        backgroundColor = .black
    }

    required init?(coder: NSCoder) { nil }

    public override var isAccessibilityElement: Bool { get { true } set {} }
    public override var accessibilityLabel: String? { get { "Scrollback" } set {} }
    /// The lines on screen, so VoiceOver reads what the scroll position shows.
    public override var accessibilityValue: String? {
        get {
            guard let terminal else { return nil }
            let lines = String(decoding: terminal.getBufferAsData(kind: .normal), as: UTF8.self)
                .split(separator: "\n", omittingEmptySubsequences: false)
            let top = min(lines.count, max(0, Int((terminal.contentOffset.y / lineHeight).rounded())))
            let rows = terminal.terminalDimensions.rows
            return lines[top..<min(lines.count, top + rows)].joined(separator: "\n")
        }
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
        lineHeight = cell.height
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
        next.alwaysBounceVertical = true
        next.onPulledPastEnd = { [weak self] in self?.onLive() }
        addSubview(next)
        if next.terminalDimensions.cols != cols || next.terminalDimensions.rows != rows {
            next.resize(cols: cols, rows: rows)
        }
        next.feed(byteArray: [UInt8](history.terminalBytes)[...])
        terminal = next
        next.layoutIfNeeded()
        show(pointsAboveNewest: distance)
        next.onOffset = { [weak self] in self?.offsetChanged() }
    }

    /// Scrolls to this many points above the newest line, or to the oldest one.
    public func show(pointsAboveNewest points: CGFloat) {
        distance = max(0, points)
        guard let terminal else { return }
        terminal.contentOffset.y = max(0, terminal.newestOffset - distance)
    }

    private func offsetChanged() {
        guard let terminal else { return }
        distance = max(0, terminal.newestOffset - terminal.contentOffset.y)
        // Within half a line counts as the newest line: deceleration stops on any point.
        onScroll(distance < lineHeight / 2)
    }
}

/// The emulator for reading: it scrolls, but never takes the keyboard.
private final class ReadOnlyTerminal: TerminalView {
    var onOffset: () -> Void = {}
    var onPulledPastEnd: () -> Void = {}
    private var pulled = false
    /// A drag this far past the newest line returns to the live pane.
    private static let pullDistance: CGFloat = 60

    override var canBecomeFirstResponder: Bool { false }

    override var contentOffset: CGPoint {
        didSet {
            if oldValue != contentOffset { onOffset() }
            guard isDragging else {
                pulled = false
                return
            }
            guard !pulled, contentOffset.y > newestOffset + Self.pullDistance else { return }
            pulled = true
            onPulledPastEnd()
        }
    }

    /// The offset that shows the newest line at the bottom.
    var newestOffset: CGFloat { max(0, contentSize.height - bounds.height) }
}
#endif
