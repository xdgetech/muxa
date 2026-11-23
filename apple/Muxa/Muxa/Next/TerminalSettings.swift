import CoreText
import Foundation
import MuxaSession
import Observation

/// How terminals look on this device: the colors (built in, or imported from
/// iTerm2 or Ghostty), the font and, on the Mac, its size. Kept in UserDefaults;
/// it never goes to the host.
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

    private(set) var themeID: String
    private(set) var imported: [TerminalTheme]
    /// A PostScript name; empty means the system monospace, which shows no icons.
    private(set) var fontName: String
    private(set) var fontSize: Double

    private let defaults: UserDefaults

    init(defaults: UserDefaults = .standard) {
        self.defaults = defaults
        themeID = defaults.string(forKey: Keys.theme) ?? TerminalTheme.muxa.id
        imported = defaults.data(forKey: Keys.imported)
            .flatMap { try? JSONDecoder().decode([TerminalTheme].self, from: $0) } ?? []
        fontName = defaults.string(forKey: Keys.font) ?? Self.bundledFont
        let size = defaults.double(forKey: Keys.size)
        fontSize = Self.sizes.contains(size) ? size : Self.defaultSize
    }

    var themes: [TerminalTheme] { TerminalTheme.builtIn + imported }

    var theme: TerminalTheme { themes.first { $0.id == themeID } ?? .muxa }

    func pick(_ theme: TerminalTheme) {
        themeID = theme.id
        defaults.set(theme.id, forKey: Keys.theme)
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

    /// Reads an iTerm2 `.itermcolors` preset, else a Ghostty theme, and picks it.
    func importTheme(from url: URL) throws {
        let data = try Data(contentsOf: url)
        let name = url.deletingPathExtension().lastPathComponent
        let theme = url.pathExtension.lowercased() == "itermcolors"
            ? try TerminalTheme.iTerm(data, name: name)
            : try TerminalTheme.ghostty(String(decoding: data, as: UTF8.self), name: name)
        imported.append(theme)
        saveImported()
        pick(theme)
    }

    func remove(_ theme: TerminalTheme) {
        imported.removeAll { $0.id == theme.id }
        saveImported()
        if themeID == theme.id { pick(.muxa) }
    }

    private func saveImported() {
        do {
            defaults.set(try JSONEncoder().encode(imported), forKey: Keys.imported)
        } catch {
            NSLog("muxa: could not save the imported terminal themes: %@", String(describing: error))
        }
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
        static let theme = "terminal.theme"
        static let imported = "terminal.imported"
        static let font = "terminal.font"
        static let size = "terminal.size"
    }
}
