import Foundation
import MuxaSession
import Testing

/// ADR 0091: theme files, their resolution, and the cases of
/// `Fixtures/themes.json`, which the Kotlin tests check too.
@Suite struct ThemeRegistryTests {
    struct Fixture: Decodable {
        var mix: [[Mixed]]
        var contrast: [[Mixed]]
        var tokenOrder: [String]
        var cases: [Case]
    }

    struct Case: Decodable {
        var resolve: String
        var id: String
        var name: String
        var appearance: String
        var tokens: [String: String]
    }

    /// A row of `mix` or `contrast`: colors, a percent, a ratio.
    enum Mixed: Decodable {
        case text(String), number(Double)

        init(from decoder: Decoder) throws {
            let c = try decoder.singleValueContainer()
            if let s = try? c.decode(String.self) { self = .text(s) } else { self = .number(try c.decode(Double.self)) }
        }

        var color: RGB? { if case .text(let s) = self { RGB(hex: s) } else { nil } }
        var number: Double? { if case .number(let n) = self { n } else { nil } }
    }

    static func fixtureData() throws -> Data {
        let url = try #require(Bundle.module.url(forResource: "themes", withExtension: "json", subdirectory: "Fixtures"))
        return try Data(contentsOf: url)
    }

    /// The bundled themes plus the fixture's `themes`.
    static func registry() throws -> ThemeRegistry {
        let raw = try #require(try JSONSerialization.jsonObject(with: fixtureData()) as? [String: Any])
        let themes = try #require(raw["themes"] as? [Any])
        let user = try themes.map { try ThemeFile.parse(JSONSerialization.data(withJSONObject: $0)) }
        return ThemeRegistry(bundled: try ThemeRegistry.bundledFiles(), user: user)
    }

    @Test func mixAndContrastMatchTheFixture() throws {
        let fixture = try JSONDecoder().decode(Fixture.self, from: Self.fixtureData())
        for row in fixture.mix {
            let (a, b, p, want) = (try #require(row[0].color), try #require(row[1].color),
                                   try #require(row[2].number), try #require(row[3].color))
            #expect(a.mixed(toward: b, Int(p)) == want, "\(a.hex) → \(b.hex) \(p)%")
        }
        for row in fixture.contrast {
            let (a, b, want) = (try #require(row[0].color), try #require(row[1].color), try #require(row[2].number))
            #expect(abs(a.contrast(with: b) - want) < 0.01, "\(a.hex) on \(b.hex)")
        }
    }

    @Test func everyCaseResolvesToTheFixturesBytes() throws {
        let fixture = try JSONDecoder().decode(Fixture.self, from: Self.fixtureData())
        #expect(fixture.tokenOrder == ThemeToken.allCases.map(\.rawValue))
        let registry = try Self.registry()
        #expect(fixture.cases.count == 10)
        for c in fixture.cases {
            let theme = registry.resolve(c.resolve)
            #expect(theme.id == c.id, "\(c.resolve)")
            #expect(theme.name == c.name, "\(c.resolve)")
            #expect(theme.appearance.rawValue == c.appearance, "\(c.resolve)")
            for token in ThemeToken.allCases {
                #expect(theme[token].hex == c.tokens[token.rawValue], "\(c.resolve) \(token)")
            }
        }
    }

    @Test func muxaDarkPinsEveryToken() throws {
        let file = try #require(try ThemeRegistry.bundledFiles().first { $0.id == ThemeRegistry.defaultID })
        let tokens = file.tokens ?? [:]
        #expect(Set(tokens.keys) == Set(ThemeToken.allCases.map(\.rawValue)))
        let theme = ThemeRegistry(bundled: [file]).resolve("muxa-dark")
        for token in ThemeToken.allCases {
            #expect(theme[token] == RGB(hex: tokens[token.rawValue] ?? ""), "\(token)")
        }
        #expect(theme.terminal.ansi == nil)
    }

    @Test func builtInsKeepTheContrastLimits() throws {
        let registry = ThemeRegistry(bundled: try ThemeRegistry.bundledFiles())
        #expect(registry.files.map(\.id) == ["muxa-dark", "ghostty", "solarized-dark", "dracula"])
        for file in registry.files {
            let t = registry.resolve(file.id)
            let ground = t[.ground]
            for token in [ThemeToken.text, .textSecondary, .failure] {
                #expect(t[token].contrast(with: ground) >= 4.5, "\(file.id) \(token)")
            }
            for token in [ThemeToken.textTertiary, .accent, .working, .needsYou, .decide] {
                #expect(t[token].contrast(with: ground) >= 3, "\(file.id) \(token)")
            }
        }
    }

    @Test func anOverrideFlowsIntoTheTokensBuiltOnIt() throws {
        let registry = try Self.registry()
        let (base, pink) = (registry.resolve("dracula"), registry.resolve("dracula-pink"))
        #expect(pink[.accent] == RGB(hex: "#ff79c6"))
        for token in [ThemeToken.done, .infoBack, .infoText, .readText, .readBack] {
            #expect(pink[token] != base[token], "\(token)")
        }
        #expect(pink[.working] == base[.working])
        #expect(pink.terminal.background == base.terminal.background)
    }

    @Test func oldIDsAndUnknownIDsStillResolve() throws {
        let registry = try Self.registry()
        #expect(registry.resolve("muxa").id == "muxa-dark")
        #expect(registry.resolve("no-such-theme").id == "muxa-dark")
        #expect(registry.resolve(.fixed("dracula"), system: .light).id == "dracula")
        #expect(registry.resolve(.system(light: "paper", dark: "dracula"), system: .light).id == "paper")
    }

    @Test func aUserFileCannotReplaceABuiltIn() throws {
        let fake = ThemeFile(id: "dracula", name: "Not Dracula",
                             terminal: .init(background: "#000000", foreground: "#ffffff"))
        let registry = ThemeRegistry(bundled: try ThemeRegistry.bundledFiles(), user: [fake])
        #expect(registry.resolve("dracula").name == "Dracula")
    }

    @Test func refusesFilesItCannotRead() {
        func parse(_ json: String) -> ThemeFile.LoadError? {
            do { _ = try ThemeFile.parse(Data(json.utf8)); return nil } catch { return error as? ThemeFile.LoadError }
        }
        let terminal = ##""terminal": {"background": "#000000", "foreground": "#ffffff"}"##
        #expect(parse("not json") == .notJSON)
        #expect(parse(##"{"schema": 2, "id": "a", "name": "A", \##(terminal)}"##) == .newerSchema(2))
        #expect(parse(##"{"schema": 1, "id": "../a", "name": "A", \##(terminal)}"##) == .badID("../a"))
        #expect(parse(##"{"schema": 1, "id": "a", "name": " ", \##(terminal)}"##) == .noName)
        #expect(parse(##"{"schema": 1, "id": "a", "name": "A"}"##) == .noTerminal)
        #expect(parse(##"{"schema": 1, "id": "a", "name": "A", \##(terminal), "tokens": {"accent": "blue"}}"##)
                == .badColor("accent"))
        #expect(parse(##"{"schema": 1, "id": "a", "name": "A", "extends": "dracula", "tokens": {"glow": "x"}}"##) == nil)
    }

    @Test func savesAndLoadsUserFiles() throws {
        let folder = FileManager.default.temporaryDirectory.appendingPathComponent("muxa-themes-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: folder) }
        let none = ThemeRegistry.userFiles(in: folder)
        #expect(none.files.isEmpty && none.failures.isEmpty)

        let imported = try TerminalTheme.ghostty("background = #101010\nforeground = #e0e0e0", name: "Mine")
        let file = ThemeFile(terminalOnly: imported)
        try ThemeRegistry.save(file, in: folder)
        try Data("{".utf8).write(to: folder.appendingPathComponent("broken.json"))

        let (files, failures) = ThemeRegistry.userFiles(in: folder)
        #expect(files == [file])
        #expect(failures.map(\.0.lastPathComponent) == ["broken.json"])
        let theme = ThemeRegistry(bundled: try ThemeRegistry.bundledFiles(), user: files).resolve(imported.id)
        #expect(theme.name == "Mine")
        #expect(theme[.ground] == RGB(hex: "#101010"))
    }

    @Test func aPickRoundTrips() throws {
        for pick in [ThemePick.fixed("dracula"), .system(light: "paper", dark: "muxa-dark")] {
            let data = try JSONEncoder().encode(pick)
            #expect(try JSONDecoder().decode(ThemePick.self, from: data) == pick)
        }
        #expect(try JSONDecoder().decode(ThemePick.self, from: Data(#"{"fixed": "ghostty"}"#.utf8)) == .fixed("ghostty"))
    }
}
