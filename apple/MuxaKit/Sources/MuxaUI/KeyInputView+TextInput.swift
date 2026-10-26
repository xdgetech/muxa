#if canImport(UIKit)
import MuxaSession
import UIKit

/// `UITextInput` over a document that holds only the composition: the pane's
/// own text lives in tmux, so there is nothing else to select or edit here.
/// Offsets are UTF-16, as in `setMarkedText`'s selected range.
extension KeyInputView {
    private var length: Int { composition.marked.utf16.count }

    public func setMarkedText(_ markedText: String?, selectedRange: NSRange) {
        let text = markedText ?? ""
        let end = text.utf16.count
        // NSNotFound: no selection; the caret goes after the text.
        let selection = selectedRange.location == NSNotFound
            ? end..<end : selectedRange.location..<(selectedRange.location + min(selectedRange.length, end))
        composition.mark(text, selection: selection)
        showComposition()
    }

    public func unmarkText() {
        send(composition.commit())
    }

    public var markedTextRange: UITextRange? {
        composition.isComposing ? Span(0, length) : nil
    }

    public var selectedTextRange: UITextRange? {
        get { Span(composition.selection.lowerBound, composition.selection.upperBound) }
        // The input method moves its selection through setMarkedText.
        set {}
    }

    public func text(in range: UITextRange) -> String? {
        guard let range = range as? Span else { return nil }
        let utf16 = composition.marked.utf16
        let lower = utf16.index(utf16.startIndex, offsetBy: min(range.lower.offset, length))
        let upper = utf16.index(utf16.startIndex, offsetBy: min(range.upper.offset, length))
        return String(utf16[lower..<upper])
    }

    public func replace(_ range: UITextRange, withText text: String) {
        insertText(text)
    }

    public var beginningOfDocument: UITextPosition { Offset(0) }
    public var endOfDocument: UITextPosition { Offset(length) }

    public func textRange(from fromPosition: UITextPosition, to toPosition: UITextPosition) -> UITextRange? {
        guard let from = fromPosition as? Offset, let to = toPosition as? Offset else { return nil }
        return Span(from.offset, to.offset)
    }

    public func position(from position: UITextPosition, offset: Int) -> UITextPosition? {
        guard let position = position as? Offset else { return nil }
        let target = position.offset + offset
        return (0...length).contains(target) ? Offset(target) : nil
    }

    public func position(from position: UITextPosition, in direction: UITextLayoutDirection, offset: Int) -> UITextPosition? {
        switch direction {
        case .left, .up: self.position(from: position, offset: -offset)
        default: self.position(from: position, offset: offset)
        }
    }

    public func compare(_ position: UITextPosition, to other: UITextPosition) -> ComparisonResult {
        let a = (position as? Offset)?.offset ?? 0
        let b = (other as? Offset)?.offset ?? 0
        return a < b ? .orderedAscending : a > b ? .orderedDescending : .orderedSame
    }

    public func offset(from: UITextPosition, to toPosition: UITextPosition) -> Int {
        ((toPosition as? Offset)?.offset ?? 0) - ((from as? Offset)?.offset ?? 0)
    }

    public func position(within range: UITextRange, farthestIn direction: UITextLayoutDirection) -> UITextPosition? {
        switch direction {
        case .left, .up: range.start
        default: range.end
        }
    }

    public func characterRange(byExtending position: UITextPosition, in direction: UITextLayoutDirection) -> UITextRange? {
        switch direction {
        case .left, .up: textRange(from: beginningOfDocument, to: position)
        default: textRange(from: position, to: endOfDocument)
        }
    }

    public func baseWritingDirection(for position: UITextPosition, in direction: UITextStorageDirection) -> NSWritingDirection {
        .leftToRight
    }

    public func setBaseWritingDirection(_ writingDirection: NSWritingDirection, for range: UITextRange) {}

    /// Where the input method puts its candidate list: under the composition.
    public func firstRect(for range: UITextRange) -> CGRect {
        composition.isComposing ? compositionLabel.frame : caretFrame()
    }

    public func caretRect(for position: UITextPosition) -> CGRect {
        let caret = caretFrame()
        let x = composition.isComposing ? compositionLabel.frame.maxX : caret.minX
        return CGRect(x: x, y: caret.minY, width: 2, height: caret.height)
    }

    public func selectionRects(for range: UITextRange) -> [UITextSelectionRect] { [] }

    public func closestPosition(to point: CGPoint) -> UITextPosition? { endOfDocument }

    public func closestPosition(to point: CGPoint, within range: UITextRange) -> UITextPosition? { range.end }

    public func characterRange(at point: CGPoint) -> UITextRange? { nil }
}

/// A UTF-16 offset into the composition. Compared by value: UIKit reads the
/// selection again and again, and a new object each time looks like a new selection.
final class Offset: UITextPosition {
    let offset: Int
    init(_ offset: Int) { self.offset = offset }
    override func isEqual(_ object: Any?) -> Bool { (object as? Offset)?.offset == offset }
    override var hash: Int { offset }
}

final class Span: UITextRange {
    let lower: Offset
    let upper: Offset
    init(_ a: Int, _ b: Int) {
        lower = Offset(min(a, b))
        upper = Offset(max(a, b))
    }
    override var start: UITextPosition { lower }
    override var end: UITextPosition { upper }
    override var isEmpty: Bool { lower.offset == upper.offset }
    override func isEqual(_ object: Any?) -> Bool {
        guard let other = object as? Span else { return false }
        return other.lower.offset == lower.offset && other.upper.offset == upper.offset
    }
    override var hash: Int { lower.offset &* 31 &+ upper.offset }
}
#endif
