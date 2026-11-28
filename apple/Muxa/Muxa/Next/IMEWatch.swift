#if os(iOS)
import MuxaSession
import UIKit

/// Follows the input method in the field being edited, so a Return that only
/// confirms a conversion does not send. UIKit posts a text change for each
/// composing step and for the confirm, before SwiftUI's `onChange` runs.
@MainActor
final class IMEWatch {
    static let shared = IMEWatch()

    private var state = IMEReturn()
    private weak var field: (any UITextInput)?

    private init() {
        for name in [UITextView.textDidChangeNotification, UITextField.textDidChangeNotification] {
            NotificationCenter.default.addObserver(
                self, selector: #selector(textDidChange(_:)), name: name, object: nil)
        }
    }

    /// Starts watching; a field calls it when it appears, before any typing.
    static func start() { _ = shared }

    /// Whether a Return now belongs to the input method.
    var ownsReturn: Bool {
        state.ownsReturn(marked: field?.markedTextRange != nil, at: ProcessInfo.processInfo.systemUptime)
    }

    @objc private func textDidChange(_ note: Notification) {
        guard let input = note.object as? any UITextInput else { return }
        field = input
        state.changed(marked: input.markedTextRange != nil, at: ProcessInfo.processInfo.systemUptime)
    }
}
#endif
