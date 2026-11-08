import Foundation

/// A pane's scrollback and screen, read once from the host (capability `history`).
public struct PaneHistory: Equatable, Sendable {
    public var pane: String
    /// Every line is this many columns wide.
    public var cols: Int
    /// Lines oldest first, each ending in `\n`, with colors as escape sequences.
    public var data: Data
    /// The host keeps older lines than were sent.
    public var truncated: Bool

    public init(pane: String, cols: Int, data: Data, truncated: Bool) {
        self.pane = pane
        self.cols = cols
        self.data = data
        self.truncated = truncated
    }

    /// How many lines `data` holds.
    public var lineCount: Int { data.reduce(0) { $1 == 0x0A ? $0 + 1 : $0 } }

    /// `data` for an emulator: each `\n` also returns to column 0.
    public var terminalBytes: Data {
        var out = Data(capacity: data.count + data.count / 40)
        for byte in data {
            if byte == 0x0A { out.append(0x0D) }
            out.append(byte)
        }
        return out
    }
}
