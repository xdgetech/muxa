# iOS text input in SwiftUI fields

The chat fields' Return (`returnSends`, `submitOnReturn` in `apple/Muxa/Muxa/Next/Platform.swift`, `IMEWatch.swift`) relies on these facts. Checked 2026-11-28 on the iOS 27.2 simulator (iPad mini). Rows marked *probe* came from a throwaway test that drove the field's `UITextInput` with `setMarkedText`, `unmarkText` and `insertText`. Rows marked *keyboard* came from a UI test that tapped the Japanese Romaji software keyboard (launch arguments `-AppleKeyboards "(ja_JP-Romaji@sw=QWERTY-Japanese;hw=Automatic)"`).

| Fact | Source |
|---|---|
| A vertical `TextField` (`axis: .vertical`) is a UITextView subclass (`VerticalTextView`). Its binding carries the marked text: each composing step reaches `onChange` while `markedTextRange` is set. | *probe*, *keyboard* |
| `UITextView.textDidChangeNotification` is posted for every composing step and for `unmarkText`, also when the confirm does not change the text. It comes before SwiftUI's `onChange` of the same edit. A confirm without a text change gives no `onChange`. | *probe* |
| On the software keyboard, the Return that confirms a conversion commits the text with no new line and no submit. The next Return types a new line. | *keyboard* |
| Not reproduced here: the user's iPad sent the message on the confirm Return. A hardware keyboard's Return could not be driven through the IME in the simulator (`typeKey` Return left the composition open). | *keyboard* |
