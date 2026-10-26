#if canImport(UIKit)
import MuxaSession
import UIKit

/// The first responder for the pane. Named keys and chords become tmux key
/// names; typed text goes through `insertText`. Cmd chords pass on to the app.
/// An input method's composition (Japanese, Chinese) is shown at the cursor and
/// sent only when committed; the text input plumbing is in KeyInputView+TextInput.
@MainActor
public final class KeyInputView: UIView, UITextInput {
    public var onOutput: (KeyMap.Output) -> Void = { _ in }
    public var onPaste: (String) -> Void = { _ in }
    /// What VoiceOver reads for the pane.
    public var screenText: () -> String = { "" }
    /// The pane's cursor cell, in this view's coordinates.
    public var caretFrame: () -> CGRect = { .zero }

    var composition = Composition()
    let compositionLabel = UILabel()
    public weak var inputDelegate: UITextInputDelegate?
    public lazy var tokenizer: UITextInputTokenizer = UITextInputStringTokenizer(textInput: self)
    public var markedTextStyle: [NSAttributedString.Key: Any]?

    public override init(frame: CGRect) {
        super.init(frame: frame)
        compositionLabel.isHidden = true
        compositionLabel.backgroundColor = .black
        addSubview(compositionLabel)
        // With a hardware keyboard, iPadOS shows these groups (input mode, dictation,
        // shortcuts) as a bar over the bottom of the pane. The pane has no use for them.
        inputAssistantItem.leadingBarButtonGroups = []
        inputAssistantItem.trailingBarButtonGroups = []
    }

    required init?(coder: NSCoder) { nil }

    public override var isAccessibilityElement: Bool { get { true } set {} }
    public override var accessibilityLabel: String? { get { "Pane" } set {} }
    public override var accessibilityValue: String? { get { screenText() } set {} }

    public override var canBecomeFirstResponder: Bool { true }
    public var hasText: Bool { true }

    public func insertText(_ text: String) {
        send(composition.insert(text))
    }

    public func deleteBackward() {
        send(composition.deleteBackward())
    }

    /// Sends what the composition gave back, and redraws the composition.
    func send(_ output: KeyMap.Output) {
        if output != .none { onOutput(output) }
        showComposition()
    }

    /// The marked text over the cursor cell, underlined, kept inside the view.
    func showComposition() {
        compositionLabel.isHidden = !composition.isComposing
        guard composition.isComposing else { return }
        let caret = caretFrame()
        // The monospaced system font's line is about 1.2 times its size: match the cell.
        let font = UIFont.monospacedSystemFont(ofSize: max(caret.height / 1.2, 8), weight: .regular)
        compositionLabel.attributedText = NSAttributedString(string: composition.marked, attributes: [
            .font: font,
            .foregroundColor: UIColor.white,
            .underlineStyle: NSUnderlineStyle.single.rawValue,
            .underlineColor: UIColor.white,
        ])
        let size = compositionLabel.sizeThatFits(bounds.size)
        let x = max(0, min(caret.minX, bounds.width - size.width))
        compositionLabel.frame = CGRect(x: x, y: caret.minY, width: size.width, height: max(size.height, caret.height))
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
    public var inlinePredictionType: UITextInlinePredictionType { get { .no } set {} }
    @available(iOS 18.0, *)
    public var writingToolsBehavior: UIWritingToolsBehavior { get { .none } set {} }

    public override func pressesBegan(_ presses: Set<UIPress>, with event: UIPressesEvent?) {
        // Return, Escape, arrows and Backspace belong to the input method while it composes.
        if composition.isComposing {
            super.pressesBegan(presses, with: event)
            return
        }
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
