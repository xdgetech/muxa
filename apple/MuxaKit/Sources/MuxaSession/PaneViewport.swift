import CoreGraphics

/// The grid a pane area shows at the default font. The bridge sizes the watched
/// pane's window for it (`viewport`, ADR 0020); the pane still renders at the
/// size it gets.
public struct PaneViewport: Equatable, Sendable {
    public var cols: Int
    public var rows: Int

    public init(cols: Int, rows: Int) {
        self.cols = cols
        self.rows = rows
    }

    /// The protocol's limits.
    static let colRange = 10...1000
    static let rowRange = 5...1000

    /// The default font, in points: a phone-sized area gets more columns from a
    /// smaller font, never below the 9 pt floor.
    public static func fontSize(compact: Bool) -> CGFloat {
        max(compact ? 10 : 13, PaneFit.minimumFontSize)
    }

    /// A pane no larger than this grid fits whole at the default font or larger,
    /// so it is shown whole: fitting the width would make it scroll for nothing.
    public func showsWhole(cols: Int, rows: Int) -> Bool {
        cols <= self.cols && rows <= self.rows
    }

    /// The cells of `cell` size that fit in `area`, with the one point of slack
    /// the fit keeps. Nil for an area with no room.
    public static func fitting(_ area: CGSize, cell: CGSize) -> PaneViewport? {
        guard area.width > 1, area.height > 1, cell.width > 0, cell.height > 0 else { return nil }
        let cols = Int(((area.width - 1) / cell.width).rounded(.down))
        let rows = Int(((area.height - 1) / cell.height).rounded(.down))
        return PaneViewport(
            cols: min(max(cols, colRange.lowerBound), colRange.upperBound),
            rows: min(max(rows, rowRange.lowerBound), rowRange.upperBound))
    }
}
