/// What Vim's window keys do with Focus's panes (ADR 0078).
public enum WindowCommand: Equatable, Sendable {
    case split, close, closeOthers, next, previous
    /// Counts from 1.
    case pane(Int)
}

/// The prefix and the key after it, as Vim's `CTRL-W` commands. Panes sit side
/// by side, so `v` and `s` both split, `h`/`l` move left and right, and `j`/`k`
/// do nothing. The prefix twice sends the prefix itself to the terminal, so a
/// program there (a shell, Vim) still gets it.
public struct WindowKeys: Sendable {
    public enum Outcome: Equatable, Sendable {
        /// The key goes to the terminal as usual.
        case pass
        /// The key is used up: the prefix starts, or an unknown key cancels it.
        case consume
        case run(WindowCommand)
    }

    public private(set) var pending = false

    public init() {}

    public mutating func press(_ chord: KeyChord, prefix: KeyChord) -> Outcome {
        guard pending else {
            if chord == prefix {
                pending = true
                return .consume
            }
            return .pass
        }
        pending = false
        if chord == prefix { return .pass }
        return Self.command(after: chord).map(Outcome.run) ?? .consume
    }

    /// Clears the prefix, as when the terminal loses the keyboard.
    public mutating func cancel() { pending = false }

    /// Vim takes `CTRL-W v` and `CTRL-W CTRL-V` alike, so Control is ignored.
    static func command(after chord: KeyChord) -> WindowCommand? {
        let mods = chord.modifiers.subtracting(.control)
        switch chord.key {
        case .named(.left): return mods.isEmpty ? .previous : nil
        case .named(.right): return mods.isEmpty ? .next : nil
        case .named: return nil
        case .character(let ch):
            if mods == .shift { return ch == "w" ? .previous : nil }
            guard mods.isEmpty else { return nil }
            switch ch {
            case "v", "s": return .split
            case "c", "q": return .close
            case "o": return .closeOthers
            case "l", "w": return .next
            case "h", "p": return .previous
            default:
                guard let n = ch.wholeNumberValue, (1...9).contains(n) else { return nil }
                return .pane(n)
            }
        }
    }
}
