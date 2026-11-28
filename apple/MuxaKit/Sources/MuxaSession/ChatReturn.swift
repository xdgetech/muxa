import Foundation

/// What a typed Return does in a chat field: it sends, or it only confirms
/// the input method's conversion (Japanese kana to kanji) and sends nothing.
public enum ChatReturn: Equatable, Sendable {
    /// Send the message; `text` is the field without the typed new line.
    case send(String)
    /// The Return confirmed a conversion: the field keeps `text` and nothing is sent.
    case confirm(String)

    /// The Return typed into `old` to make `new`, or nil for any other edit:
    /// typing, a paste or a deletion. `imeOwnsReturn` comes from `IMEReturn`.
    public static func typed(old: String, new: String, imeOwnsReturn: Bool) -> ChatReturn? {
        guard let rest = withoutTypedReturn(old: old, new: new) else { return nil }
        return imeOwnsReturn ? .confirm(rest) : .send(rest)
    }

    /// `new` without the one new line typed into `old`; nil for any other edit.
    static func withoutTypedReturn(old: String, new: String) -> String? {
        guard new.count == old.count + 1 else { return nil }
        let at = zip(old, new).prefix { $0 == $1 }.count
        let i = new.index(new.startIndex, offsetBy: at)
        guard new[i] == "\n" else { return nil }
        var rest = new
        rest.remove(at: i)
        return rest == old ? rest : nil
    }
}

/// Whether a Return belongs to the input method. It does while the field has
/// marked text, and for a moment after the conversion is confirmed, so a new
/// line or a submit from the same key press does not send.
public struct IMEReturn: Equatable, Sendable {
    /// Seconds after a confirm. A Return from the same key press comes well
    /// inside it; a second press by the user comes after it.
    public static let confirmWindow: TimeInterval = 0.1

    private var marked = false
    private var confirmedAt: TimeInterval?

    public init() {}

    /// After every change of the field's text, marked or not. `time` is the system uptime.
    public mutating func changed(marked: Bool, at time: TimeInterval) {
        if self.marked && !marked { confirmedAt = time }
        self.marked = marked
    }

    /// `marked`: the field has marked text now.
    public func ownsReturn(marked: Bool, at time: TimeInterval) -> Bool {
        if marked { return true }
        guard let confirmedAt else { return false }
        return time - confirmedAt < Self.confirmWindow
    }
}
