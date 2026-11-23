import Foundation

/// An sRGB color, 8 bits a channel.
public struct RGB: Hashable, Codable, Sendable {
    public var r: UInt8, g: UInt8, b: UInt8

    public init(_ r: UInt8, _ g: UInt8, _ b: UInt8) {
        (self.r, self.g, self.b) = (r, g, b)
    }

    /// `#1d1f21` or `1d1f21`.
    public init?(hex: String) {
        let digits = hex.hasPrefix("#") ? hex.dropFirst() : Substring(hex)
        guard digits.count == 6, let v = UInt32(digits, radix: 16) else { return nil }
        self.init(UInt8(v >> 16 & 0xFF), UInt8(v >> 8 & 0xFF), UInt8(v & 0xFF))
    }

    /// Components from 0 to 1, as iTerm2 stores them.
    init(unit r: Double, _ g: Double, _ b: Double) {
        func byte(_ x: Double) -> UInt8 { UInt8((min(max(x, 0), 1) * 255).rounded()) }
        self.init(byte(r), byte(g), byte(b))
    }

    public var hex: String { String(format: "#%02x%02x%02x", r, g, b) }
}

/// A terminal's colors: what the user picks on each device, built in or
/// imported from iTerm2 or Ghostty.
public struct TerminalTheme: Hashable, Codable, Sendable, Identifiable {
    public var id: String
    public var name: String
    public var background: RGB
    public var foreground: RGB
    public var cursor: RGB?
    public var selection: RGB?
    /// The 16 ANSI colors; nil keeps the terminal view's own.
    public var ansi: [RGB]?

    public init(id: String, name: String, background: RGB, foreground: RGB, cursor: RGB? = nil,
                selection: RGB? = nil, ansi: [RGB]? = nil) {
        self.id = id
        self.name = name
        self.background = background
        self.foreground = foreground
        self.cursor = cursor
        self.selection = selection
        self.ansi = ansi
    }

    public enum ImportError: Error, Equatable, LocalizedError {
        case notAPlist
        case missing(String)

        public var errorDescription: String? {
            switch self {
            case .notAPlist: "The file is not an iTerm2 color preset (.itermcolors)."
            case .missing(let what): "The theme has no \(what)."
            }
        }
    }
}

// MARK: Built in

extension TerminalTheme {
    /// The design mocks' terminal, with the terminal view's own ANSI colors.
    public static let muxa = TerminalTheme(
        id: "muxa", name: "muxa", background: RGB(0x0A, 0x0C, 0x0F), foreground: RGB(0xD5, 0xD9, 0xDF))

    /// Ghostty's default colors (`ghostty +show-config --default`).
    public static let ghostty = TerminalTheme(
        id: "ghostty", name: "Ghostty", background: hex("282c34"), foreground: hex("ffffff"),
        ansi: palette("1d1f21 cc6666 b5bd68 f0c674 81a2be b294bb 8abeb7 c5c8c6 666666 d54e53 b9ca4a e7c547 7aa6da c397d8 70c0b1 eaeaea"))

    public static let solarizedDark = TerminalTheme(
        id: "solarized-dark", name: "Solarized Dark", background: hex("002b36"), foreground: hex("839496"),
        cursor: hex("93a1a1"), selection: hex("073642"),
        ansi: palette("073642 dc322f 859900 b58900 268bd2 d33682 2aa198 eee8d5 002b36 cb4b16 586e75 657b83 839496 6c71c4 93a1a1 fdf6e3"))

    public static let dracula = TerminalTheme(
        id: "dracula", name: "Dracula", background: hex("282a36"), foreground: hex("f8f8f2"),
        cursor: hex("f8f8f2"), selection: hex("44475a"),
        ansi: palette("21222c ff5555 50fa7b f1fa8c bd93f9 ff79c6 8be9fd f8f8f2 6272a4 ff6e6e 69ff94 ffffa5 d6acff ff92df a4ffff ffffff"))

    public static let builtIn: [TerminalTheme] = [.muxa, .ghostty, .solarizedDark, .dracula]

    private static func hex(_ s: String) -> RGB { RGB(hex: s) ?? RGB(0, 0, 0) }
    private static func palette(_ s: String) -> [RGB] { s.split(separator: " ").map { hex(String($0)) } }
}

// MARK: Import

extension TerminalTheme {
    /// An iTerm2 color preset: a plist with `Ansi 0 Color` … `Ansi 15 Color`,
    /// `Background Color` and `Foreground Color`, each with components from 0 to 1.
    public static func iTerm(_ data: Data, name: String) throws -> TerminalTheme {
        guard let plist = try? PropertyListSerialization.propertyList(from: data, format: nil) as? [String: Any] else {
            throw ImportError.notAPlist
        }
        func color(_ key: String) -> RGB? {
            guard let c = plist[key] as? [String: Any],
                  let r = c["Red Component"] as? Double, let g = c["Green Component"] as? Double,
                  let b = c["Blue Component"] as? Double
            else { return nil }
            return RGB(unit: r, g, b)
        }
        guard let background = color("Background Color") else { throw ImportError.missing("background color") }
        guard let foreground = color("Foreground Color") else { throw ImportError.missing("foreground color") }
        let ansi = (0..<16).compactMap { color("Ansi \($0) Color") }
        return TerminalTheme(
            id: "import-\(UUID().uuidString)", name: name, background: background, foreground: foreground,
            cursor: color("Cursor Color"), selection: color("Selection Color"), ansi: ansi.count == 16 ? ansi : nil)
    }

    /// A Ghostty theme or config: `background = #282c34`, `foreground = …`,
    /// `cursor-color = …`, `selection-background = …` and `palette = N=#rrggbb`.
    /// Other keys and `#` comments are skipped.
    public static func ghostty(_ text: String, name: String) throws -> TerminalTheme {
        var values: [String: String] = [:]
        var palette: [Int: RGB] = [:]
        for raw in text.split(whereSeparator: \.isNewline) {
            let line = raw.trimmingCharacters(in: .whitespaces)
            guard !line.hasPrefix("#"), let eq = line.firstIndex(of: "=") else { continue }
            let key = line[..<eq].trimmingCharacters(in: .whitespaces)
            let value = line[line.index(after: eq)...].trimmingCharacters(in: .whitespaces)
            if key == "palette", let split = value.firstIndex(of: "="),
               let n = Int(value[..<split].trimmingCharacters(in: .whitespaces)), (0..<16).contains(n),
               let c = RGB(hex: value[value.index(after: split)...].trimmingCharacters(in: .whitespaces)) {
                palette[n] = c
            } else {
                values[key] = value
            }
        }
        func color(_ key: String) -> RGB? { values[key].flatMap(RGB.init(hex:)) }
        guard let background = color("background") else { throw ImportError.missing("background color") }
        guard let foreground = color("foreground") else { throw ImportError.missing("foreground color") }
        let ansi = (0..<16).compactMap { palette[$0] }
        return TerminalTheme(
            id: "import-\(UUID().uuidString)", name: name, background: background, foreground: foreground,
            cursor: color("cursor-color"), selection: color("selection-background"), ansi: ansi.count == 16 ? ansi : nil)
    }
}
