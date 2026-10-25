/// Turns a hardware key press into what the bridge sends. Named keys go to
/// tmux `send-keys`, so tmux encodes them for the pane's modes; text is typed literally.
public enum KeyMap {
    public enum Special: Equatable, Sendable {
        case up, down, left, right, home, end, pageUp, pageDown
        case enter, escape, tab, backspace, deleteForward, insert
        case function(Int)
    }

    public struct Modifiers: OptionSet, Sendable {
        public let rawValue: Int
        public init(rawValue: Int) { self.rawValue = rawValue }
        public static let control = Modifiers(rawValue: 1)
        public static let option = Modifiers(rawValue: 2)
        public static let shift = Modifiers(rawValue: 4)
        public static let command = Modifiers(rawValue: 8)
    }

    public enum Output: Equatable, Sendable {
        case keys([String])
        case text(String)
        /// Not for the pane: an app shortcut, or a key tmux cannot name.
        case none
    }

    /// `characters` is what the key types without modifiers applied by the OS
    /// (`UIKey.charactersIgnoringModifiers`).
    public static func map(special: Special?, characters: String, modifiers: Modifiers) -> Output {
        if modifiers.contains(.command) { return .none }
        if let special {
            guard let name = name(special, shift: modifiers.contains(.shift)) else { return .none }
            return .keys([prefix(modifiers, shiftInName: special == .tab) + name])
        }
        guard modifiers.contains(.control) || modifiers.contains(.option) else {
            return characters.isEmpty ? .none : .text(characters)
        }
        // tmux names one printable ASCII character after the modifiers.
        guard characters.count == 1, let scalar = characters.unicodeScalars.first,
            scalar.isASCII, scalar.value >= 0x21, scalar.value < 0x7f
        else {
            return characters == " " ? .keys([prefix(modifiers, shiftInName: false) + "Space"]) : .none
        }
        // Shift is already in the character (`A`, `!`); naming it again changes the key.
        return .keys([prefix(modifiers.subtracting(.shift), shiftInName: false) + characters])
    }

    private static func prefix(_ m: Modifiers, shiftInName: Bool) -> String {
        var p = ""
        if m.contains(.control) { p += "C-" }
        if m.contains(.option) { p += "M-" }
        if m.contains(.shift) && !shiftInName { p += "S-" }
        return p
    }

    private static func name(_ key: Special, shift: Bool) -> String? {
        switch key {
        case .up: "Up"
        case .down: "Down"
        case .left: "Left"
        case .right: "Right"
        case .home: "Home"
        case .end: "End"
        case .pageUp: "PageUp"
        case .pageDown: "PageDown"
        case .enter: "Enter"
        case .escape: "Escape"
        case .tab: shift ? "BTab" : "Tab"
        case .backspace: "BSpace"
        case .deleteForward: "DC"
        case .insert: "IC"
        case .function(let n): (1...12).contains(n) ? "F\(n)" : nil
        }
    }
}
