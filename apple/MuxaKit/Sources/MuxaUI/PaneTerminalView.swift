#if canImport(UIKit)
import MuxaSession
import SwiftTerm
import UIKit

/// Shows a pane at exactly its cols x rows, with the largest font that fits.
/// It never asks for another size: the pane belongs to the desktop layout.
@MainActor
public final class PaneTerminalView: UIView, PaneRenderer {
    /// `.whole` fits the whole grid in the view. `.width` fits the columns only:
    /// the grid is then taller than the view, and a scroll view around it shows a part.
    public enum FitMode: Sendable {
        case whole, width
    }

    public let terminalView: TerminalView
    public private(set) var cols = 80
    public private(set) var rows = 24
    public var fitMode: FitMode = .whole {
        didSet { if fitMode != oldValue { setNeedsLayout() } }
    }
    /// Called after new bytes or a new screen, so a scroll view can follow the cursor.
    public var onUpdate: (_ reset: Bool) -> Void = { _ in }
    /// Called when a new screen brings a new grid size, before it is fitted and fed:
    /// a resize after the feed would clear the screen.
    public var onGridChange: () -> Void = {}
    /// The grid had to be reset after a screen was fed, which cleared it: ask the
    /// bridge for the screen again.
    public var onScreenLost: () -> Void = {}
    private var fed = false
    private let replies = DropReplies()
    private var fitted: (size: CGSize, cols: Int, rows: Int, mode: FitMode)?

    public override init(frame: CGRect) {
        terminalView = TerminalView(frame: .zero, font: nil, options: TerminalOptions(cols: 80, rows: 24, scrollback: 0))
        super.init(frame: frame)
        terminalView.terminalDelegate = replies
        // Keys come from KeyInputView; the terminal view never becomes first responder.
        terminalView.isUserInteractionEnabled = false
        backgroundColor = .black
        addSubview(terminalView)
    }

    required init?(coder: NSCoder) { nil }

    public func reset(cols: Int, rows: Int, data: Data) {
        let changed = max(cols, 1) != self.cols || max(rows, 1) != self.rows
        self.cols = max(cols, 1)
        self.rows = max(rows, 1)
        if changed { onGridChange() }
        fit(force: true)
        // Before any layout the view has no size and fit does nothing; the grid must
        // still be right before the feed, when resizing clears nothing.
        let grid = terminalView.terminalDimensions
        if grid.cols != self.cols || grid.rows != self.rows {
            terminalView.resize(cols: self.cols, rows: self.rows)
        }
        terminalView.feed(byteArray: [UInt8](data)[...])
        fed = true
        onUpdate(true)
    }

    public func feed(_ data: Data) {
        terminalView.feed(byteArray: [UInt8](data)[...])
        onUpdate(false)
    }

    /// The cursor cell, in this view's coordinates.
    public var cursorFrame: CGRect {
        terminalView.convert(terminalView.caretFrame, to: self)
    }

    /// The height the grid needs at `width` in `.width` mode.
    public func gridHeight(forWidth width: CGFloat) -> CGFloat {
        let metrics = CellMetrics(scale: displayScale)
        let size = CGSize(width: width, height: .greatestFiniteMagnitude)
        let font = UIFont.monospacedSystemFont(
            ofSize: metrics.largestFontSize(cols: cols, rows: nil, in: size), weight: .regular)
        return (metrics.cell(for: font).height * CGFloat(rows)).rounded(.up) + 1
    }

    private var displayScale: CGFloat {
        max(window?.screen.scale ?? traitCollection.displayScale, 1)
    }

    /// The screen as text, for VoiceOver and UI tests.
    public var screenText: String {
        String(decoding: terminalView.getBufferAsData(kind: .active), as: UTF8.self)
    }

    public override func layoutSubviews() {
        super.layoutSubviews()
        fit(force: false)
    }

    /// Picks the font, then sets the terminal view's frame to exactly the grid.
    private func fit(force: Bool) {
        let size = bounds.size
        guard size.width > 0, size.height > 0 else { return }
        if !force, let f = fitted, f.size == size, f.cols == cols, f.rows == rows, f.mode == fitMode { return }
        fitted = (size, cols, rows, fitMode)

        let metrics = CellMetrics(scale: displayScale)
        let fontSize = metrics.largestFontSize(cols: cols, rows: fitMode == .whole ? rows : nil, in: size)
        let font = UIFont.monospacedSystemFont(ofSize: fontSize, weight: .regular)
        let cell = metrics.cell(for: font)

        // Less than one cell of slack, so SwiftTerm's floor(frame / cell) is exactly the grid.
        let width = cell.width * CGFloat(cols) + min(0.5, cell.width / 2)
        let height = cell.height * CGFloat(rows) + min(0.5, cell.height / 2)
        terminalView.frame = CGRect(
            x: ((size.width - width) / 2).rounded(.down), y: 0, width: width, height: height)
        // The font last: setting it recomputes cols and rows from the frame just set.
        if terminalView.font.pointSize != fontSize { terminalView.font = font }

        // A fallback. It soft-resets, which clears a screen already fed.
        let grid = terminalView.terminalDimensions
        if grid.cols != cols || grid.rows != rows {
            terminalView.resize(cols: cols, rows: rows)
            if fed {
                fed = false
                onScreenLost()
            }
        }
    }
}

/// Cell size for a font, computed as SwiftTerm does (`computeFontDimensions`):
/// the width of "W" rounded to the pixel grid, the line height rounded up.
struct CellMetrics {
    let scale: CGFloat

    func cell(for font: UIFont) -> CGSize {
        let width = "W".size(withAttributes: [.font: font]).width
        let height = ceil(CTFontGetAscent(font) + CTFontGetDescent(font) + CTFontGetLeading(font))
        return CGSize(
            width: max(1, (width * scale).rounded() / scale),
            height: max(1, ceil(height * scale) / scale))
    }

    /// With `rows` nil, only the width limits the font.
    func largestFontSize(cols: Int, rows: Int?, in size: CGSize) -> CGFloat {
        func fits(_ s: CGFloat) -> Bool {
            let c = cell(for: UIFont.monospacedSystemFont(ofSize: s, weight: .regular))
            guard c.width * CGFloat(cols) + 1 <= size.width else { return false }
            guard let rows else { return true }
            return c.height * CGFloat(rows) + 1 <= size.height
        }
        // Cell size grows with the font size, so search it.
        var low: CGFloat = 2, high: CGFloat = 64
        guard fits(low) else { return low }
        while high - low > 0.25 {
            let mid = (low + high) / 2
            if fits(mid) { low = mid } else { high = mid }
        }
        return low
    }
}

/// The emulator's replies to host queries (device attributes, cursor position)
/// must not reach the pane: tmux answers those itself. Size changes are ignored.
private final class DropReplies: TerminalViewDelegate {
    func send(source: TerminalView, data: ArraySlice<UInt8>) {}
    func sizeChanged(source: TerminalView, newCols: Int, newRows: Int) {}
    func setTerminalTitle(source: TerminalView, title: String) {}
    func hostCurrentDirectoryUpdate(source: TerminalView, directory: String?) {}
    func scrolled(source: TerminalView, position: Double) {}
    func requestOpenLink(source: TerminalView, link: String, params: [String: String]) {}
    func bell(source: TerminalView) {}
    func clipboardCopy(source: TerminalView, content: Data) {}
    func iTermContent(source: TerminalView, content: ArraySlice<UInt8>) {}
    func rangeChanged(source: TerminalView, startY: Int, endY: Int) {}
}
#endif
