/// A key with its modifiers: how the Mac app stores, shows and matches a shortcut.
public struct KeyChord: Hashable, Codable, Sendable {
    public struct Modifiers: OptionSet, Hashable, Codable, Sendable {
        public let rawValue: Int
        public init(rawValue: Int) { self.rawValue = rawValue }
        public static let control = Modifiers(rawValue: 1)
        public static let option = Modifiers(rawValue: 2)
        public static let command = Modifiers(rawValue: 4)
        public static let shift = Modifiers(rawValue: 8)
    }

    /// Keys without a printable character.
    public enum Named: String, Codable, Sendable, CaseIterable {
        case tab, space, `return`, delete, escape, left, right, up, down
        case f1, f2, f3, f4, f5, f6, f7, f8, f9, f10, f11, f12
    }

    public enum Key: Hashable, Codable, Sendable {
        /// One character as typed without Shift: `d`, `[`, `\`, `1`.
        case character(Character)
        case named(Named)

        private enum CodingKeys: String, CodingKey { case character, named }

        public init(from decoder: Decoder) throws {
            let c = try decoder.container(keyedBy: CodingKeys.self)
            if let named = try c.decodeIfPresent(Named.self, forKey: .named) {
                self = .named(named)
                return
            }
            let text = try c.decode(String.self, forKey: .character)
            guard text.count == 1, let first = text.first else {
                throw DecodingError.dataCorruptedError(forKey: .character, in: c, debugDescription: "one character")
            }
            self = .character(first)
        }

        public func encode(to encoder: Encoder) throws {
            var c = encoder.container(keyedBy: CodingKeys.self)
            switch self {
            case .character(let ch): try c.encode(String(ch), forKey: .character)
            case .named(let n): try c.encode(n, forKey: .named)
            }
        }
    }

    public var key: Key
    public var modifiers: Modifiers

    public init(_ key: Key, _ modifiers: Modifiers = []) {
        // Letters are stored lowercase; Shift is a modifier.
        if case .character(let ch) = key, ch.isUppercase {
            self.key = .character(Character(ch.lowercased()))
        } else {
            self.key = key
        }
        self.modifiers = modifiers
    }

    public init(_ character: Character, _ modifiers: Modifiers = []) {
        self.init(.character(character), modifiers)
    }

    public init(_ named: Named, _ modifiers: Modifiers = []) {
        self.init(.named(named), modifiers)
    }

    /// As the app prints it, in the app's order: `⌃⌥Space`, `⌘⇧H`, `F8`.
    public var display: String {
        var out = ""
        if modifiers.contains(.control) { out += "⌃" }
        if modifiers.contains(.option) { out += "⌥" }
        if modifiers.contains(.command) { out += "⌘" }
        if modifiers.contains(.shift) { out += "⇧" }
        switch key {
        case .character(let ch): out += ch.uppercased()
        case .named(let n): out += Self.glyph(n)
        }
        return out
    }

    private static func glyph(_ n: Named) -> String {
        switch n {
        case .tab: "Tab"
        case .space: "Space"
        case .return: "⏎"
        case .delete: "⌫"
        case .escape: "Esc"
        case .left: "←"
        case .right: "→"
        case .up: "↑"
        case .down: "↓"
        default: n.rawValue.uppercased()
        }
    }
}
