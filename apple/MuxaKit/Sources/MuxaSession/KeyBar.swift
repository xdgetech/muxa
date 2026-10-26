/// The keys above the on-screen keyboard, which cannot type them itself.
/// Ctrl is one-shot: it applies to the next key or character, then releases.
public struct KeyBar: Equatable, Sendable {
    public enum Key: CaseIterable, Sendable {
        case escape, tab, backTab, control, left, down, up, right, interrupt
    }

    public private(set) var control = false

    public init() {}

    /// A tap on one of the bar's keys.
    public mutating func tap(_ key: Key) -> KeyMap.Output {
        let mods: KeyMap.Modifiers = control ? [.control] : []
        let output: KeyMap.Output
        switch key {
        case .control:
            control.toggle()
            return .none
        case .interrupt: output = .keys(["C-c"])
        case .escape: output = KeyMap.map(special: .escape, characters: "", modifiers: mods)
        case .tab: output = KeyMap.map(special: .tab, characters: "", modifiers: mods)
        case .backTab: output = KeyMap.map(special: .tab, characters: "", modifiers: mods.union(.shift))
        case .left: output = KeyMap.map(special: .left, characters: "", modifiers: mods)
        case .down: output = KeyMap.map(special: .down, characters: "", modifiers: mods)
        case .up: output = KeyMap.map(special: .up, characters: "", modifiers: mods)
        case .right: output = KeyMap.map(special: .right, characters: "", modifiers: mods)
        }
        control = false
        return output
    }

    /// What `type` would return, without releasing Ctrl.
    public func peek(_ output: KeyMap.Output) -> KeyMap.Output {
        var copy = self
        return copy.type(output)
    }

    /// What the keyboard typed, with a pending Ctrl applied. Ctrl names only one
    /// ASCII character, so other text goes as typed and still releases it.
    public mutating func type(_ output: KeyMap.Output) -> KeyMap.Output {
        guard control, output != .none else { return output }
        control = false
        guard case .text(let text) = output else { return output }
        let chord = KeyMap.map(special: nil, characters: text, modifiers: [.control])
        return chord == .none ? output : chord
    }
}
