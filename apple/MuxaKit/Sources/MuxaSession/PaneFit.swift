import CoreGraphics

/// How a pane's grid is fitted to the view. No mode changes the grid: it stays
/// the pane's cols x rows (ADR 0004).
public enum PaneFit: Sendable, Hashable {
    /// Every row and column in the view.
    case whole
    /// The columns fill the width. The grid can be taller than the view, which scrolls.
    case width
    /// The font never goes below `minimumFontSize`. The grid can be wider and
    /// taller than the view, which scrolls on both axes.
    case readable

    /// The smallest font a phone screen shows legibly, in points.
    public static let minimumFontSize: CGFloat = 9

    /// The mode a view picks for itself, from the largest fonts at which the whole
    /// grid, and the columns alone, fit. Wide views fit the width.
    public static func automatic(wide: Bool, wholeFont: CGFloat, widthFont: CGFloat) -> PaneFit {
        if !wide, wholeFont >= minimumFontSize { return .whole }
        return widthFont >= minimumFontSize ? .width : .readable
    }

    /// The font size this mode uses, from the same two fonts.
    public func fontSize(wholeFont: CGFloat, widthFont: CGFloat) -> CGFloat {
        switch self {
        case .whole: wholeFont
        case .width: widthFont
        case .readable: max(widthFont, Self.minimumFontSize)
        }
    }

    /// A pinch: in for the whole pane, out for a readable font.
    public static func pinched(scale: CGFloat) -> PaneFit? {
        if scale < 0.8 { return .whole }
        if scale > 1.25 { return .readable }
        return nil
    }
}
