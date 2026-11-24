import Foundation

/// A theme with every token set: what the apps draw with.
public struct ResolvedTheme: Hashable, Sendable, Identifiable {
    public let id: String
    public let name: String
    public let appearance: ThemeAppearance
    /// The terminal's colors; `ansi` nil keeps the terminal view's own.
    public let terminal: TerminalTheme
    private let colors: [ThemeToken: RGB]

    init(id: String, name: String, appearance: ThemeAppearance, terminal: TerminalTheme, colors: [ThemeToken: RGB]) {
        self.id = id
        self.name = name
        self.appearance = appearance
        self.terminal = terminal
        self.colors = colors
    }

    public subscript(_ token: ThemeToken) -> RGB { colors[token] ?? terminal.foreground }
}

/// The themes a device can pick: the bundled files first, then the user's.
/// The first file with an id wins, so a user file cannot replace a built-in.
public struct ThemeRegistry: Sendable {
    public static let defaultID = "muxa-dark"
    /// ADR 0080's ids, still saved on devices.
    public static let aliases = ["muxa": "muxa-dark"]

    public let files: [ThemeFile]
    public let bundledIDs: Set<String>
    private let byID: [String: ThemeFile]

    public init(bundled: [ThemeFile], user: [ThemeFile] = []) {
        var byID: [String: ThemeFile] = [:]
        var files: [ThemeFile] = []
        for file in bundled + user where byID[file.id] == nil {
            byID[file.id] = file
            files.append(file)
        }
        self.files = files
        self.byID = byID
        bundledIDs = Set(bundled.map(\.id))
    }

    public func file(_ id: String) -> ThemeFile? { byID[Self.aliases[id] ?? id] }

    /// Always gives a theme: an unknown id resolves as the default.
    public func resolve(_ id: String) -> ResolvedTheme {
        guard let file = file(id) ?? file(Self.defaultID) else {
            return Self.derive(chain: [ThemeFile(terminalOnly: .muxa)])
        }
        return Self.derive(chain: chain(of: file))
    }

    public func resolve(_ pick: ThemePick, system: ThemeAppearance) -> ResolvedTheme {
        resolve(pick.id(system: system))
    }

    /// The file and its parents, the root first. A cycle or an unknown parent
    /// ends the chain; a chain with no terminal colors starts from the default.
    func chain(of file: ThemeFile) -> [ThemeFile] {
        var links = [file]
        var seen: Set<String> = [file.id]
        while let parentID = links[0].extends, let parent = self.file(parentID), !seen.contains(parent.id) {
            links.insert(parent, at: 0)
            seen.insert(parent.id)
        }
        guard !links.contains(where: { $0.terminal != nil }) else { return links }
        guard let root = self.file(Self.defaultID), !seen.contains(root.id) else {
            return [ThemeFile(terminalOnly: .muxa)] + links
        }
        return chain(of: root) + links
    }

    private static let muxaTerminal = ThemeFile.Terminal(
        background: TerminalTheme.muxa.background.hex, foreground: TerminalTheme.muxa.foreground.hex)

    /// ADR 0091 decision 3: the last terminal palette in the chain, then each
    /// token in order, where an override replaces it before later tokens read it.
    static func derive(chain: [ThemeFile]) -> ResolvedTheme {
        let me = chain[chain.count - 1]
        let source = chain.last { $0.terminal != nil }?.terminal ?? Self.muxaTerminal
        let terminal = TerminalTheme(
            id: me.id, name: me.name,
            background: RGB(hex: source.background) ?? TerminalTheme.muxa.background,
            foreground: RGB(hex: source.foreground) ?? TerminalTheme.muxa.foreground,
            cursor: source.cursor.flatMap(RGB.init(hex:)), selection: source.selection.flatMap(RGB.init(hex:)),
            ansi: source.ansi.map { $0.compactMap(RGB.init(hex:)) }.flatMap { $0.count == 16 ? $0 : nil })
        let palette = ThemePalette(background: terminal.background, foreground: terminal.foreground, ansi: terminal.ansi)

        var overrides: [ThemeToken: RGB] = [:]
        for file in chain {
            for (key, value) in file.tokens ?? [:] {
                if let token = ThemeToken(rawValue: key), let color = RGB(hex: value) { overrides[token] = color }
            }
        }
        var colors: [ThemeToken: RGB] = [:]
        for token in ThemeToken.allCases {
            colors[token] = overrides[token] ?? token.derive(palette) { colors[$0] ?? palette.foreground }
        }
        let stated = chain.reversed().lazy.compactMap { $0.appearance.flatMap(ThemeAppearance.init(rawValue:)) }.first
        let measured: ThemeAppearance = terminal.background.luminance < terminal.foreground.luminance ? .dark : .light
        return ResolvedTheme(id: me.id, name: me.name, appearance: stated ?? measured, terminal: terminal, colors: colors)
    }
}

// MARK: Editing colors

extension ThemeRegistry {
    /// The user theme that keeps color edits to `id` (ADR 0091 T4): a user
    /// theme that already extends another is edited in place; any other gets
    /// a `<id>-custom` file that extends it, made once and reused.
    public func editable(_ id: String) -> ThemeFile {
        let base = file(id) ?? file(Self.defaultID) ?? ThemeFile(terminalOnly: .muxa)
        if !bundledIDs.contains(base.id), base.extends != nil { return base }
        let customID = "\(base.id)-custom"
        if let existing = file(customID), !bundledIDs.contains(customID) { return existing }
        return ThemeFile(id: customID, name: "\(base.name) (custom)", extends: base.id)
    }
}

extension ThemeFile {
    /// This file with `token` set to `color`, or back to its parent's when nil.
    public func setting(_ token: ThemeToken, to color: RGB?) -> ThemeFile {
        var copy = self
        var tokens = tokens ?? [:]
        tokens[token.rawValue] = color?.hex
        copy.tokens = tokens.isEmpty ? nil : tokens
        return copy
    }
}

// MARK: Files

extension ThemeRegistry {
    /// The themes bundled with the apps, in `themes/index.json` order.
    public static func bundledFiles() throws -> [ThemeFile] {
        guard let index = Bundle.module.url(forResource: "index", withExtension: "json", subdirectory: "themes") else {
            throw CocoaError(.fileNoSuchFile)
        }
        let ids = try JSONDecoder().decode([String].self, from: Data(contentsOf: index))
        return try ids.map { id in
            guard let url = Bundle.module.url(forResource: id, withExtension: "json", subdirectory: "themes") else {
                throw CocoaError(.fileNoSuchFile, userInfo: [NSFilePathErrorKey: "themes/\(id).json"])
            }
            return try ThemeFile.parse(Data(contentsOf: url))
        }
    }

    /// The user's theme files in `folder`, by name. A file that does not load
    /// is returned in `failures` with its error, so the caller can log it.
    public static func userFiles(in folder: URL) -> (files: [ThemeFile], failures: [(URL, Error)]) {
        let urls: [URL]
        do {
            urls = try FileManager.default.contentsOfDirectory(at: folder, includingPropertiesForKeys: nil)
        } catch CocoaError.fileReadNoSuchFile {
            return ([], [])
        } catch {
            return ([], [(folder, error)])
        }
        var files: [ThemeFile] = []
        var failures: [(URL, Error)] = []
        for url in urls.filter({ $0.pathExtension == "json" }).sorted(by: { $0.lastPathComponent < $1.lastPathComponent }) {
            do {
                files.append(try ThemeFile.parse(Data(contentsOf: url)))
            } catch {
                failures.append((url, error))
            }
        }
        return (files, failures)
    }

    /// Writes `file` as `<id>.json` in `folder`, replacing one with that id.
    public static func save(_ file: ThemeFile, in folder: URL) throws {
        try file.check()
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        try file.encoded().write(to: folder.appendingPathComponent("\(file.id).json"), options: .atomic)
    }
}

/// What the device picked (ADR 0091 decision 7): one theme, or one for each
/// system appearance.
public enum ThemePick: Hashable, Sendable, Codable {
    case fixed(String)
    case system(light: String, dark: String)

    public func id(system: ThemeAppearance) -> String {
        switch self {
        case .fixed(let id): id
        case .system(let light, let dark): system == .light ? light : dark
        }
    }

    private enum Keys: String, CodingKey { case fixed, system }
    private struct Pair: Codable { var light: String, dark: String }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: Keys.self)
        if let pair = try c.decodeIfPresent(Pair.self, forKey: .system) {
            self = .system(light: pair.light, dark: pair.dark)
        } else {
            self = .fixed(try c.decode(String.self, forKey: .fixed))
        }
    }

    public func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: Keys.self)
        switch self {
        case .fixed(let id): try c.encode(id, forKey: .fixed)
        case .system(let light, let dark): try c.encode(Pair(light: light, dark: dark), forKey: .system)
        }
    }
}
