import Foundation
import MuxaSession
import Testing

@Suite struct TerminalThemeTests {
    @Test func hexReadsWithOrWithoutHash() {
        #expect(RGB(hex: "#282c34") == RGB(0x28, 0x2c, 0x34))
        #expect(RGB(hex: "ffffff")?.hex == "#ffffff")
        #expect(RGB(hex: "#fff") == nil)
        #expect(RGB(hex: "zzzzzz") == nil)
    }

    @Test func builtInThemesHaveSixteenColorsOrNone() {
        for t in TerminalTheme.builtIn {
            #expect(t.ansi == nil || t.ansi?.count == 16, "\(t.name)")
        }
        #expect(Set(TerminalTheme.builtIn.map(\.id)).count == TerminalTheme.builtIn.count)
    }

    @Test func readsAGhosttyTheme() throws {
        let text = """
        # a comment
        background = 282c34
        foreground = #ffffff
        cursor-color = #aeafad
        font-size = 13
        """ + "\n" + (0..<16).map { "palette = \($0)=#0000\(String(format: "%02x", $0))" }.joined(separator: "\n")
        let t = try TerminalTheme.ghostty(text, name: "Mine")
        #expect(t.name == "Mine")
        #expect(t.background == RGB(0x28, 0x2c, 0x34))
        #expect(t.cursor == RGB(0xae, 0xaf, 0xad))
        #expect(t.ansi?.count == 16)
        #expect(t.ansi?[15] == RGB(0, 0, 15))
    }

    @Test func aGhosttyThemeWithoutColorsIsRefused() {
        #expect(throws: TerminalTheme.ImportError.missing("background color")) {
            try TerminalTheme.ghostty("font-size = 13", name: "x")
        }
    }

    @Test func readsAniTermPreset() throws {
        func color(_ r: Double, _ g: Double, _ b: Double) -> [String: Any] {
            ["Red Component": r, "Green Component": g, "Blue Component": b, "Color Space": "sRGB"]
        }
        var plist: [String: Any] = ["Background Color": color(0, 0, 0), "Foreground Color": color(1, 1, 1),
                                    "Cursor Color": color(1, 0, 0)]
        for n in 0..<16 { plist["Ansi \(n) Color"] = color(Double(n) / 15, 0, 0) }
        let data = try PropertyListSerialization.data(fromPropertyList: plist, format: .xml, options: 0)
        let t = try TerminalTheme.iTerm(data, name: "Preset")
        #expect(t.foreground == RGB(255, 255, 255))
        #expect(t.cursor == RGB(255, 0, 0))
        #expect(t.ansi?[15] == RGB(255, 0, 0))
        #expect(throws: TerminalTheme.ImportError.notAPlist) { try TerminalTheme.iTerm(Data("nope".utf8), name: "x") }
    }

    @Test func survivesJSON() throws {
        let data = try JSONEncoder().encode(TerminalTheme.dracula)
        #expect(try JSONDecoder().decode(TerminalTheme.self, from: data) == .dracula)
    }
}
