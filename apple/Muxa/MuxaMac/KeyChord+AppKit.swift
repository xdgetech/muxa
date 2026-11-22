import AppKit
import MuxaSession
import SwiftUI

extension KeyChord {
    /// The chord a key press makes; nil for a modifier alone or a key the app does not name.
    init?(_ event: NSEvent) {
        let flags = event.modifierFlags.intersection(.deviceIndependentFlagsMask)
        var mods: Modifiers = []
        if flags.contains(.control) { mods.insert(.control) }
        if flags.contains(.option) { mods.insert(.option) }
        if flags.contains(.command) { mods.insert(.command) }
        if flags.contains(.shift) { mods.insert(.shift) }
        if let named = Self.named[event.keyCode] {
            self.init(named, mods)
            return
        }
        // The key as typed with no modifiers: `[` for ⌘⇧[, `w` for ⌃W.
        guard let text = event.characters(byApplyingModifiers: []), text.count == 1, let ch = text.first,
              !ch.isWhitespace, !ch.isNewline
        else { return nil }
        self.init(ch, mods)
    }

    var keyboardShortcut: KeyboardShortcut {
        var mods: EventModifiers = []
        if modifiers.contains(.control) { mods.insert(.control) }
        if modifiers.contains(.option) { mods.insert(.option) }
        if modifiers.contains(.command) { mods.insert(.command) }
        if modifiers.contains(.shift) { mods.insert(.shift) }
        return KeyboardShortcut(keyEquivalent, modifiers: mods)
    }

    private var keyEquivalent: KeyEquivalent {
        switch key {
        case .character(let ch): return KeyEquivalent(ch)
        case .named(let n):
            switch n {
            case .tab: return .tab
            case .space: return .space
            case .return: return .return
            case .delete: return .delete
            case .escape: return .escape
            case .left: return .leftArrow
            case .right: return .rightArrow
            case .up: return .upArrow
            case .down: return .downArrow
            default:
                // f1…f12: the number after "f".
                let number = Int(n.rawValue.dropFirst()) ?? 1
                return KeyEquivalent(Character(UnicodeScalar(NSF1FunctionKey + number - 1) ?? " "))
            }
        }
    }

    /// Virtual key codes of the keys without a character (Carbon's `kVK_*`).
    private static let named: [UInt16: Named] = [
        48: .tab, 49: .space, 36: .return, 76: .return, 51: .delete, 53: .escape,
        123: .left, 124: .right, 125: .down, 126: .up,
        122: .f1, 120: .f2, 99: .f3, 118: .f4, 96: .f5, 97: .f6,
        98: .f7, 100: .f8, 101: .f9, 109: .f10, 103: .f11, 111: .f12,
    ]
}
