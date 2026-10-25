import Foundation
import SwiftTerm

/// Where a watched pane's bytes go. The app's terminal view is one; `HeadlessScreen` is another.
@MainActor
public protocol PaneRenderer: AnyObject {
    /// Called for every `screen`: a fresh emulator of exactly this size, then `data`.
    func reset(cols: Int, rows: Int, data: Data)
    func feed(_ data: Data)
}

/// A SwiftTerm emulator with no view. Replies the emulator would send to the
/// host (device attributes, cursor position) are dropped: tmux answers those itself.
@MainActor
public final class HeadlessScreen: PaneRenderer {
    public private(set) var terminal: Terminal
    private let sink = DropReplies()

    public init() {
        terminal = Terminal(delegate: sink, options: TerminalOptions(cols: 1, rows: 1, scrollback: 0))
    }

    public func reset(cols: Int, rows: Int, data: Data) {
        terminal = Terminal(
            delegate: sink, options: TerminalOptions(cols: max(cols, 1), rows: max(rows, 1), scrollback: 0))
        terminal.feed(byteArray: [UInt8](data))
    }

    public func feed(_ data: Data) {
        terminal.feed(byteArray: [UInt8](data))
    }

    /// Visible rows with trailing spaces and trailing empty rows removed.
    /// Cells never written read as `\0`; they show as spaces.
    public var rows: [String] {
        var rows = (0..<terminal.rows).map { row -> String in
            let text = terminal.getLine(row: row)?.translateToString(skipNullCellsFollowingWide: true) ?? ""
            var line = text.replacingOccurrences(of: "\0", with: " ")
            while line.last == " " { line.removeLast() }
            return line
        }
        while rows.last?.isEmpty == true { rows.removeLast() }
        return rows
    }

    /// Zero-based (row, column), as the scenario fixtures store it.
    public var cursor: (row: Int, col: Int) {
        let (x, y) = terminal.getCursorLocation()
        return (y, x)
    }
}

final class DropReplies: TerminalDelegate {
    func showCursor(source: Terminal) {}
    func hideCursor(source: Terminal) {}
    func send(source: Terminal, data: ArraySlice<UInt8>) {}
    func scrolled(source: Terminal, yDisp: Int) {}
    func linefeed(source: Terminal) {}
    func mouseModeChanged(source: Terminal) {}
    func colorChanged(source: Terminal, idx: Int?) {}
    func iTermContent(source: Terminal, content: ArraySlice<UInt8>) {}
    func clipboardCopy(source: Terminal, content: Data) {}
    func notify(source: Terminal, title: String, body: String) {}
    func selectionChanged(source: Terminal) {}
}
