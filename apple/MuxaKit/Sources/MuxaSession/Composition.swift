/// Text an input method is still composing (marked text, as in Japanese kana
/// before conversion). It stays on the iPad: only committed text reaches the pane.
public struct Composition: Equatable, Sendable {
    public private(set) var marked = ""
    /// The input method's selection inside `marked`, in UTF-16 offsets.
    public private(set) var selection = 0..<0

    public init() {}

    public var isComposing: Bool { !marked.isEmpty }

    public mutating func mark(_ text: String, selection: Range<Int>) {
        marked = text
        let length = text.utf16.count
        self.selection = min(selection.lowerBound, length)..<min(selection.upperBound, length)
    }

    /// Ends the composition and returns what to send: the marked text as typed.
    public mutating func commit() -> KeyMap.Output {
        let text = marked
        mark("", selection: 0..<0)
        return text.isEmpty ? .none : .text(text)
    }

    /// Text that replaces the composition, or plain typing when there is none.
    public mutating func insert(_ text: String) -> KeyMap.Output {
        mark("", selection: 0..<0)
        // A hardware Return arrives as "\n" when pressesBegan let it through.
        if text == "\n" { return .keys(["Enter"]) }
        return text.isEmpty ? .none : .text(text)
    }

    /// Inside a composition, Backspace edits the composition and sends nothing.
    public mutating func deleteBackward() -> KeyMap.Output {
        guard isComposing else { return .keys(["BSpace"]) }
        let text = String(marked.dropLast())
        mark(text, selection: text.utf16.count..<text.utf16.count)
        return .none
    }
}
