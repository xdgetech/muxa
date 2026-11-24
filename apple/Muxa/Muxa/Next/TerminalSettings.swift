import CoreText
import Foundation
import MuxaSession
import Observation

/// The terminal's font on this device and, on the Mac, its size. Kept in
/// UserDefaults; it never goes to the host. The colors come from the device's
/// theme (`ThemeStore`, ADR 0091).
@MainActor
@Observable
final class TerminalSettings {
    static let shared = TerminalSettings()

    /// Bundled with the apps (`scripts/vendor-fonts.sh`): Ghostty's default
    /// font, and the Nerd Font symbols that prompts and TUIs print as icons.
    static let bundledFont = "JetBrainsMono-Regular"
    static let symbolsFont = "SymbolsNFM"
    static let defaultSize = 13.0
    static let sizes = 9.0...24.0

    /// A PostScript name; empty means the system monospace, which shows no icons.
    private(set) var fontName: String
    private(set) var fontSize: Double

    private let defaults: UserDefaults

    init(defaults: UserDefaults = .standard) {
        self.defaults = defaults
        fontName = defaults.string(forKey: Keys.font) ?? Self.bundledFont
        let size = defaults.double(forKey: Keys.size)
        fontSize = Self.sizes.contains(size) ? size : Self.defaultSize
    }

    func setFont(_ name: String) {
        fontName = name
        defaults.set(name, forKey: Keys.font)
    }

    func setSize(_ size: Double) {
        fontSize = min(max(size, Self.sizes.lowerBound), Self.sizes.upperBound)
        defaults.set(fontSize, forKey: Keys.size)
    }

    /// The picked font at `size`, with the Nerd Font symbols as its fallback;
    /// nil for the system monospace. A cascade list on a named font survives
    /// SwiftTerm's bold and italic variants; the system monospace ignores it.
    func font(size: Double) -> CTFont? {
        guard !fontName.isEmpty else { return nil }
        let base = CTFontDescriptorCreateWithNameAndSize(fontName as CFString, 0)
        let cascade = [CTFontDescriptorCreateWithNameAndSize(Self.symbolsFont as CFString, 0)]
        let descriptor = CTFontDescriptorCreateCopyWithAttributes(
            base, [kCTFontCascadeListAttribute: cascade] as CFDictionary)
        return CTFontCreateWithFontDescriptor(descriptor, CGFloat(size), nil)
    }

    /// Makes the bundled fonts usable by name in this process. Called once at launch.
    static func registerFonts() {
        let urls = Bundle.main.urls(forResourcesWithExtension: "ttf", subdirectory: nil) ?? []
        CTFontManagerRegisterFontURLs(urls as CFArray, .process, true) { errors, _ in
            for case let error as NSError in errors as? [Any] ?? [] where error.code != 105 {
                // 105: already registered, as after a relaunch in the same process.
                NSLog("muxa: a bundled font was not registered: %@", error.localizedDescription)
            }
            return true
        }
    }

    private enum Keys {
        static let font = "terminal.font"
        static let size = "terminal.size"
    }
}
