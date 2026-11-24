import Foundation
import MuxaSession
import Observation
import SwiftUI
#if canImport(UIKit)
import UIKit
#endif

/// The device's theme (ADR 0091): it colors the chrome, the terminals and the
/// Reader. The pick stays in UserDefaults; imported themes are files in
/// Application Support. Reads are locked and need no actor, so `Palette` can
/// read it from anywhere; SwiftUI still sees each read through the registrar.
final class ThemeStore: Observable, @unchecked Sendable {
    static let shared = ThemeStore()

    private let registrar = ObservationRegistrar()
    private let lock = NSLock()
    private let defaults: UserDefaults
    private let folder: URL?
    private var state: State

    private struct State {
        var registry: ThemeRegistry
        var pick: ThemePick
        var theme: ResolvedTheme
    }

    init(defaults: UserDefaults = .standard, folder: URL? = ThemeStore.defaultFolder) {
        self.defaults = defaults
        self.folder = folder
        let registry = Self.load(defaults: defaults, folder: folder)
        let pick = Self.savedPick(defaults)
        state = State(registry: registry, pick: pick, theme: registry.resolve(pick, system: .dark))
    }

    /// The picked theme, resolved.
    var theme: ResolvedTheme {
        registrar.access(self, keyPath: \.theme)
        return lock.withLock { state.theme }
    }

    /// Every theme this device can pick: the built-ins, then the user's.
    var files: [ThemeFile] {
        registrar.access(self, keyPath: \.theme)
        return lock.withLock { state.registry.files }
    }

    func isBuiltIn(_ id: String) -> Bool { lock.withLock { state.registry.bundledIDs.contains(id) } }

    func resolve(_ id: String) -> ResolvedTheme { lock.withLock { state.registry.resolve(id) } }

    func color(_ token: ThemeToken) -> Color { Color(theme[token]) }

    func pick(_ id: String) {
        let pick = ThemePick.fixed(id)
        do {
            defaults.set(try JSONEncoder().encode(pick), forKey: Keys.pick)
        } catch {
            NSLog("muxa: could not save the theme pick: %@", String(describing: error))
        }
        update { $0.pick = pick }
    }

    /// Shows a theme without saving the pick: snapshots of each theme.
    func preview(_ id: String) {
        update { $0.pick = .fixed(id) }
    }

    /// Reads a muxa theme file, an iTerm2 `.itermcolors` preset or a Ghostty
    /// theme, saves it as a theme file, and picks it.
    func importTheme(from url: URL) throws {
        guard let folder else { throw CocoaError(.fileWriteNoPermission) }
        let data = try Data(contentsOf: url)
        let name = url.deletingPathExtension().lastPathComponent
        let file: ThemeFile
        switch url.pathExtension.lowercased() {
        case "json": file = try ThemeFile.parse(data)
        case "itermcolors": file = ThemeFile(terminalOnly: try TerminalTheme.iTerm(data, name: name))
        default: file = ThemeFile(terminalOnly: try TerminalTheme.ghostty(String(decoding: data, as: UTF8.self), name: name))
        }
        guard !isBuiltIn(file.id) else { throw ThemeFile.LoadError.badID(file.id) }
        try ThemeRegistry.save(file, in: folder)
        reload()
        pick(file.id)
    }

    /// Removes a user theme: its file, or an import kept by ADR 0080.
    func remove(_ id: String) {
        if let folder {
            let url = folder.appendingPathComponent("\(id).json")
            do {
                if FileManager.default.fileExists(atPath: url.path) { try FileManager.default.removeItem(at: url) }
            } catch {
                NSLog("muxa: could not remove the theme %@: %@", id, String(describing: error))
            }
        }
        let legacy = Self.legacyImports(defaults).filter { $0.id != id }
        do {
            defaults.set(try JSONEncoder().encode(legacy), forKey: Keys.legacyImported)
        } catch {
            NSLog("muxa: could not save the imported terminal themes: %@", String(describing: error))
        }
        reload()
        if lock.withLock({ state.pick.id(system: .dark) }) == id { pick(ThemeRegistry.defaultID) }
    }

    private func reload() {
        let registry = Self.load(defaults: defaults, folder: folder)
        update { $0.registry = registry }
    }

    private func update(_ change: (inout State) -> Void) {
        registrar.withMutation(of: self, keyPath: \.theme) {
            lock.withLock {
                change(&state)
                state.theme = state.registry.resolve(state.pick, system: .dark)
            }
        }
    }

    // MARK: Loading

    static var defaultFolder: URL? {
        FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first?
            .appendingPathComponent("Themes", isDirectory: true)
    }

    private static func load(defaults: UserDefaults, folder: URL?) -> ThemeRegistry {
        let bundled: [ThemeFile]
        do {
            bundled = try ThemeRegistry.bundledFiles()
        } catch {
            NSLog("muxa: the bundled themes did not load: %@", String(describing: error))
            bundled = []
        }
        var user: [ThemeFile] = []
        if let folder {
            let (files, failures) = ThemeRegistry.userFiles(in: folder)
            user = files
            for (url, error) in failures {
                NSLog("muxa: the theme %@ did not load: %@", url.lastPathComponent, error.localizedDescription)
            }
        }
        // ADR 0080 kept imports in UserDefaults; they load as terminal-only themes.
        user += legacyImports(defaults).map(ThemeFile.init(terminalOnly:))
        return ThemeRegistry(bundled: bundled, user: user)
    }

    private static func legacyImports(_ defaults: UserDefaults) -> [TerminalTheme] {
        guard let data = defaults.data(forKey: Keys.legacyImported) else { return [] }
        do {
            return try JSONDecoder().decode([TerminalTheme].self, from: data)
        } catch {
            NSLog("muxa: the imported terminal themes did not load: %@", String(describing: error))
            return []
        }
    }

    /// The pick, else ADR 0080's terminal theme id, else the default.
    private static func savedPick(_ defaults: UserDefaults) -> ThemePick {
        if let data = defaults.data(forKey: Keys.pick) {
            do {
                return try JSONDecoder().decode(ThemePick.self, from: data)
            } catch {
                NSLog("muxa: the theme pick did not load: %@", String(describing: error))
            }
        }
        return .fixed(defaults.string(forKey: Keys.legacyTheme) ?? ThemeRegistry.defaultID)
    }

    private enum Keys {
        static let pick = "theme.pick"
        static let legacyTheme = "terminal.theme"
        static let legacyImported = "terminal.imported"
    }
}

extension Color {
    init(_ c: RGB) {
        self.init(.sRGB, red: Double(c.r) / 255, green: Double(c.g) / 255, blue: Double(c.b) / 255)
    }
}

extension ColorScheme {
    init(_ appearance: ThemeAppearance) {
        self = appearance == .light ? .light : .dark
    }
}

/// A window root in the device's theme: its accent, light or dark controls,
/// and ground. Read in a view body, so the window follows a new theme.
struct ThemedWindow: ViewModifier {
    func body(content: Content) -> some View {
        let theme = ThemeStore.shared.theme
        let ground = Color(theme[.ground])
        content
            .tint(Color(theme[.accent]))
            .background(ground)
            .preferredColorScheme(ColorScheme(theme.appearance))
            #if os(macOS)
            .containerBackground(ground, for: .window)
            .toolbarBackground(ground, for: .windowToolbar)
            #else
            .onAppear { Self.styleSegments(theme) }
            .onChange(of: theme) { _, theme in Self.styleSegments(theme) }
            #endif
    }

    #if canImport(UIKit)
    /// The Reader's native pickers take the theme's segment colors. The proxy
    /// styles segmented controls made after this call.
    @MainActor
    private static func styleSegments(_ theme: ResolvedTheme) {
        let segments = UISegmentedControl.appearance()
        segments.backgroundColor = UIColor(Color(theme[.surface]))
        segments.selectedSegmentTintColor = UIColor(Color(theme[.selected]))
    }
    #endif
}
