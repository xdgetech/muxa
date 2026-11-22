import Foundation
import MuxaSession
import Observation

/// The user's keys on this Mac (ADR 0078): a key per rebindable shortcut, and
/// the prefix of Vim's window keys, off by default. Kept in UserDefaults.
@MainActor
@Observable
final class KeyBindings {
    static let shared = KeyBindings()
    /// Vim's own window prefix, offered when the user turns the keys on.
    static let defaultPrefix = KeyChord("w", .control)

    /// Shortcut id → its key; `.none` removes a default key.
    private(set) var overrides: [String: KeyChord?]
    /// Nil while Vim's window keys are off.
    private(set) var vimPrefix: KeyChord?

    private let defaults: UserDefaults

    init(defaults: UserDefaults = .standard) {
        self.defaults = defaults
        overrides = Self.load([String: KeyChord?].self, defaults, Self.overridesKey) ?? [:]
        vimPrefix = Self.load(KeyChord.self, defaults, Self.prefixKey)
    }

    func chord(for s: Shortcut) -> KeyChord? {
        if let set = overrides[s.id] { return set }
        return s.defaultChord
    }

    func isCustom(_ s: Shortcut) -> Bool { overrides[s.id] != nil }

    /// `chord` nil leaves the shortcut without a key.
    func set(_ chord: KeyChord?, for s: Shortcut) {
        overrides[s.id] = chord == s.defaultChord ? nil : .some(chord)
        save(overrides, Self.overridesKey)
    }

    func reset(_ s: Shortcut) {
        overrides[s.id] = nil
        save(overrides, Self.overridesKey)
    }

    func resetAll() {
        overrides = [:]
        save(overrides, Self.overridesKey)
    }

    func setVimPrefix(_ chord: KeyChord?) {
        vimPrefix = chord
        if let chord { save(chord, Self.prefixKey) } else { defaults.removeObject(forKey: Self.prefixKey) }
    }

    /// The other shortcuts, and the Vim prefix, that use `chord`.
    func users(of chord: KeyChord, except id: String) -> [String] {
        var names = Shortcut.all.filter { $0.id != id && $0.rebindable && self.chord(for: $0) == chord }.map(\.title)
        if id != Self.prefixID, vimPrefix == chord { names.append("Vim window prefix") }
        return names
    }

    static let prefixID = "vim-prefix"
    private static let overridesKey = "keys.overrides"
    private static let prefixKey = "keys.vimPrefix"

    private static func load<T: Decodable>(_ type: T.Type, _ defaults: UserDefaults, _ key: String) -> T? {
        guard let data = defaults.data(forKey: key) else { return nil }
        do {
            return try JSONDecoder().decode(type, from: data)
        } catch {
            // A damaged entry falls back to the default keys.
            NSLog("muxa: ignored unreadable %@: %@", key, String(describing: error))
            return nil
        }
    }

    private func save<T: Encodable>(_ value: T, _ key: String) {
        do {
            defaults.set(try JSONEncoder().encode(value), forKey: key)
        } catch {
            NSLog("muxa: could not save %@: %@", key, String(describing: error))
        }
    }
}
