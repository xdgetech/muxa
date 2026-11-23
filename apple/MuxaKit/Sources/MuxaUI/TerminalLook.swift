#if canImport(UIKit)
import CoreText
import MuxaSession
import SwiftTerm
import UIKit

/// How a terminal draws on iOS: the user's font family and colors. The font
/// size comes from the fit, so the grid stays the terminal's real size.
public struct TerminalLook: Equatable, Sendable {
    /// A PostScript name; nil is the system monospace, which cannot fall back
    /// to the symbols font.
    public var fontName: String?
    /// Fonts for glyphs the family lacks, such as the Nerd Font icons.
    public var fallbackFontNames: [String]
    public var theme: TerminalTheme

    public init(fontName: String? = nil, fallbackFontNames: [String] = [], theme: TerminalTheme = .muxa) {
        self.fontName = fontName
        self.fallbackFontNames = fallbackFontNames
        self.theme = theme
    }

    public func font(size: CGFloat) -> UIFont {
        guard let fontName, let base = UIFont(name: fontName, size: size) else {
            return UIFont.monospacedSystemFont(ofSize: size, weight: .regular)
        }
        guard !fallbackFontNames.isEmpty else { return base }
        // A cascade list on a named font survives SwiftTerm's bold and italic variants.
        let cascade = fallbackFontNames.map { CTFontDescriptorCreateWithNameAndSize($0 as CFString, 0) }
        let descriptor = CTFontDescriptorCreateCopyWithAttributes(
            base.fontDescriptor as CTFontDescriptor, [kCTFontCascadeListAttribute: cascade] as CFDictionary)
        return CTFontCreateWithFontDescriptor(descriptor, size, nil) as UIFont
    }
}

extension UIColor {
    convenience init(_ c: RGB) {
        self.init(red: CGFloat(c.r) / 255, green: CGFloat(c.g) / 255, blue: CGFloat(c.b) / 255, alpha: 1)
    }
}

extension SwiftTerm.Color {
    convenience init(_ c: RGB) {
        self.init(red: UInt16(c.r) * 257, green: UInt16(c.g) * 257, blue: UInt16(c.b) * 257)
    }
}
#endif
