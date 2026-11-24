import Foundation

public enum ThemeAppearance: String, Codable, Sendable {
    case dark, light
}

/// One theme as a file (ADR 0091): partial, kept as written, resolved by
/// `ThemeRegistry`. Unknown keys and unknown token names are ignored, so a
/// file written for a newer app still loads.
public struct ThemeFile: Codable, Hashable, Sendable, Identifiable {
    /// Raised only for a change older apps must not read.
    public static let schemaVersion = 1

    public var schema: Int
    public var id: String
    public var name: String
    public var extends: String?
    /// `dark` or `light`; any other value is ignored.
    public var appearance: String?
    public var terminal: Terminal?
    /// Token key → `#rrggbb`.
    public var tokens: [String: String]?

    public struct Terminal: Codable, Hashable, Sendable {
        public var background: String
        public var foreground: String
        public var cursor: String?
        public var selection: String?
        /// The 16 ANSI colors, or none.
        public var ansi: [String]?

        public init(background: String, foreground: String, cursor: String? = nil,
                    selection: String? = nil, ansi: [String]? = nil) {
            self.background = background
            self.foreground = foreground
            self.cursor = cursor
            self.selection = selection
            self.ansi = ansi
        }
    }

    public init(id: String, name: String, extends: String? = nil, appearance: ThemeAppearance? = nil,
                terminal: Terminal? = nil, tokens: [String: String]? = nil) {
        schema = Self.schemaVersion
        self.id = id
        self.name = name
        self.extends = extends
        self.appearance = appearance?.rawValue
        self.terminal = terminal
        self.tokens = tokens
    }

    public enum LoadError: Error, Equatable, LocalizedError {
        case notJSON
        case newerSchema(Int)
        case badID(String)
        case noName
        case noTerminal
        case badColor(String)

        public var errorDescription: String? {
            switch self {
            case .notJSON: "The theme is not a muxa theme file (JSON)."
            case .newerSchema(let n): "The theme needs a newer muxa (theme schema \(n))."
            case .badID(let id): "The theme's id \"\(id)\" may use only letters, digits, \".\", \"_\" and \"-\"."
            case .noName: "The theme has no name."
            case .noTerminal: "The theme has no terminal colors and extends no other theme."
            case .badColor(let key): "The theme's \(key) is not a #rrggbb color."
            }
        }
    }

    /// Reads and checks a theme file.
    public static func parse(_ data: Data) throws -> ThemeFile {
        let file: ThemeFile
        do {
            file = try JSONDecoder().decode(ThemeFile.self, from: data)
        } catch {
            throw LoadError.notJSON
        }
        try file.check()
        return file
    }

    func check() throws {
        guard schema <= Self.schemaVersion else { throw LoadError.newerSchema(schema) }
        guard Self.validID(id) else { throw LoadError.badID(id) }
        guard !name.trimmingCharacters(in: .whitespaces).isEmpty else { throw LoadError.noName }
        guard terminal != nil || extends != nil else { throw LoadError.noTerminal }
        if let t = terminal {
            let colors = [("background", t.background), ("foreground", t.foreground)]
                + [("cursor", t.cursor), ("selection", t.selection)].compactMap { k, v in v.map { (k, $0) } }
            for (key, value) in colors where RGB(hex: value) == nil { throw LoadError.badColor("terminal \(key)") }
            if let ansi = t.ansi {
                guard ansi.count == 16 else { throw LoadError.badColor("terminal ansi (needs 16)") }
                for (i, value) in ansi.enumerated() where RGB(hex: value) == nil {
                    throw LoadError.badColor("terminal ansi \(i)")
                }
            }
        }
        for (key, value) in tokens ?? [:] where ThemeToken(rawValue: key) != nil && RGB(hex: value) == nil {
            throw LoadError.badColor(key)
        }
    }

    /// Ids name files in the user's theme folder.
    static func validID(_ id: String) -> Bool {
        guard !id.isEmpty, id.count <= 100, !id.hasPrefix(".") else { return false }
        return id.unicodeScalars.allSatisfy {
            CharacterSet.alphanumerics.contains($0) && $0.isASCII || "._-".unicodeScalars.contains($0)
        }
    }

    public func encoded() throws -> Data {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        return try encoder.encode(self)
    }
}

extension ThemeFile {
    /// A terminal-only theme, as ADR 0080's imports and the iTerm2 and Ghostty
    /// readers make it. The chrome is derived from it.
    public init(terminalOnly theme: TerminalTheme) {
        self.init(
            id: theme.id, name: theme.name,
            terminal: Terminal(
                background: theme.background.hex, foreground: theme.foreground.hex,
                cursor: theme.cursor?.hex, selection: theme.selection?.hex, ansi: theme.ansi?.map(\.hex)))
    }
}
