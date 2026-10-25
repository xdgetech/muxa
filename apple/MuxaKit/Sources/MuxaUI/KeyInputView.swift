#if canImport(UIKit)
import MuxaSession
import UIKit

/// The first responder for the pane. Named keys and chords become tmux key
/// names; typed text goes through `insertText`. Cmd chords pass on to the app.
@MainActor
public final class KeyInputView: UIView, UIKeyInput {
    public var onOutput: (KeyMap.Output) -> Void = { _ in }
    public var onPaste: (String) -> Void = { _ in }
    /// What VoiceOver reads for the pane.
    public var screenText: () -> String = { "" }

    public override var isAccessibilityElement: Bool { get { true } set {} }
    public override var accessibilityLabel: String? { get { "Pane" } set {} }
    public override var accessibilityValue: String? { get { screenText() } set {} }

    public override var canBecomeFirstResponder: Bool { true }
    public var hasText: Bool { true }

    public func insertText(_ text: String) {
        // A hardware Return arrives here as "\n" when pressesBegan let it through.
        onOutput(text == "\n" ? .keys(["Enter"]) : .text(text))
    }

    public func deleteBackward() {
        onOutput(.keys(["BSpace"]))
    }

    // The pane has no text to select, so Cmd+V is the only edit action.
    public override func canPerformAction(_ action: Selector, withSender sender: Any?) -> Bool {
        action == #selector(paste(_:)) && UIPasteboard.general.hasStrings
    }

    public override func paste(_ sender: Any?) {
        if let text = UIPasteboard.general.string { onPaste(text) }
    }

    // A terminal wants exactly what was typed.
    public var autocorrectionType: UITextAutocorrectionType { get { .no } set {} }
    public var autocapitalizationType: UITextAutocapitalizationType { get { .none } set {} }
    public var smartQuotesType: UITextSmartQuotesType { get { .no } set {} }
    public var smartDashesType: UITextSmartDashesType { get { .no } set {} }
    public var spellCheckingType: UITextSpellCheckingType { get { .no } set {} }

    public override func pressesBegan(_ presses: Set<UIPress>, with event: UIPressesEvent?) {
        var unhandled = Set<UIPress>()
        for press in presses {
            guard let key = press.key else {
                unhandled.insert(press)
                continue
            }
            let mods = Self.modifiers(key.modifierFlags)
            let special = Self.special(key.keyCode)
            // Plain typing goes through insertText, which also covers dead keys.
            if special == nil && !mods.contains(.control) && !mods.contains(.option) {
                unhandled.insert(press)
                continue
            }
            let output = KeyMap.map(special: special, characters: key.charactersIgnoringModifiers, modifiers: mods)
            if output == .none {
                unhandled.insert(press)
            } else {
                onOutput(output)
            }
        }
        if !unhandled.isEmpty { super.pressesBegan(unhandled, with: event) }
    }

    static func modifiers(_ flags: UIKeyModifierFlags) -> KeyMap.Modifiers {
        var m = KeyMap.Modifiers()
        if flags.contains(.control) { m.insert(.control) }
        if flags.contains(.alternate) { m.insert(.option) }
        if flags.contains(.shift) { m.insert(.shift) }
        if flags.contains(.command) { m.insert(.command) }
        return m
    }

    static func special(_ code: UIKeyboardHIDUsage) -> KeyMap.Special? {
        switch code {
        case .keyboardUpArrow: .up
        case .keyboardDownArrow: .down
        case .keyboardLeftArrow: .left
        case .keyboardRightArrow: .right
        case .keyboardHome: .home
        case .keyboardEnd: .end
        case .keyboardPageUp: .pageUp
        case .keyboardPageDown: .pageDown
        case .keyboardReturnOrEnter, .keypadEnter: .enter
        case .keyboardEscape: .escape
        case .keyboardTab: .tab
        case .keyboardDeleteOrBackspace: .backspace
        case .keyboardDeleteForward: .deleteForward
        case .keyboardInsert: .insert
        default:
            if code.rawValue >= UIKeyboardHIDUsage.keyboardF1.rawValue,
                code.rawValue <= UIKeyboardHIDUsage.keyboardF12.rawValue
            {
                .function(code.rawValue - UIKeyboardHIDUsage.keyboardF1.rawValue + 1)
            } else {
                nil
            }
        }
    }
}
#endif
