import UIKit
import WebKit

/// The Reader's web view, scrolled by a hardware keyboard. WebKit hands most key
/// presses on a page with nothing to edit back up the responder chain unread, so
/// Space, the arrows, Page Up and Down, Home and End would do nothing.
final class ReaderPageView: WKWebView {
    /// One arrow press, in points.
    private static let line: CGFloat = 40
    /// What a page step keeps of the previous screen, in points.
    private static let overlap: CGFloat = 40

    override func pressesBegan(_ presses: Set<UIPress>, with event: UIPressesEvent?) {
        var unhandled = Set<UIPress>()
        for press in presses {
            if let key = press.key, let target = offset(for: key) {
                scrollView.setContentOffset(CGPoint(x: scrollView.contentOffset.x, y: target), animated: true)
            } else {
                unhandled.insert(press)
            }
        }
        if !unhandled.isEmpty { super.pressesBegan(unhandled, with: event) }
    }

    /// Where `key` scrolls to, or nil for a key that does not scroll.
    private func offset(for key: UIKey) -> CGFloat? {
        let flags = key.modifierFlags.intersection([.shift, .command, .control, .alternate])
        let page = max(scrollView.bounds.height - scrollView.adjustedContentInset.top
            - scrollView.adjustedContentInset.bottom - Self.overlap, Self.line)
        let y = scrollView.contentOffset.y
        let delta: CGFloat
        switch (key.keyCode, flags) {
        case (.keyboardSpacebar, []), (.keyboardPageDown, []): delta = page
        case (.keyboardSpacebar, .shift), (.keyboardPageUp, []): delta = -page
        case (.keyboardDownArrow, []): delta = Self.line
        case (.keyboardUpArrow, []): delta = -Self.line
        case (.keyboardEnd, []), (.keyboardDownArrow, .command): delta = .greatestFiniteMagnitude
        case (.keyboardHome, []), (.keyboardUpArrow, .command): delta = -.greatestFiniteMagnitude
        default: return nil
        }
        let top = -scrollView.adjustedContentInset.top
        let bottom = max(top, scrollView.contentSize.height - scrollView.bounds.height + scrollView.adjustedContentInset.bottom)
        return min(max(y + delta, top), bottom)
    }
}
