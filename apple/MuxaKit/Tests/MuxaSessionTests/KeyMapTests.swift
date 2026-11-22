import Testing

@testable import MuxaSession

struct KeyMapTests {
    typealias M = KeyMap.Modifiers

    @Test func specialKeysAreNamed() {
        let cases: [(KeyMap.Special, M, String)] = [
            (.up, [], "Up"),
            (.enter, [], "Enter"),
            (.enter, [.shift], "S-Enter"),
            (.tab, [.shift], "BTab"),
            (.backspace, [], "BSpace"),
            (.left, [.control, .option], "C-M-Left"),
            (.function(5), [], "F5"),
        ]
        for (key, mods, name) in cases {
            #expect(KeyMap.map(special: key, characters: "", modifiers: mods) == .keys([name]))
        }
    }

    @Test func chordsUseTmuxNames() {
        #expect(KeyMap.map(special: nil, characters: "c", modifiers: [.control]) == .keys(["C-c"]))
        #expect(KeyMap.map(special: nil, characters: "b", modifiers: [.option]) == .keys(["M-b"]))
        #expect(KeyMap.map(special: nil, characters: "A", modifiers: [.control, .shift]) == .keys(["C-A"]))
        #expect(KeyMap.map(special: nil, characters: " ", modifiers: [.control]) == .keys(["C-Space"]))
    }

    /// With two keyboard languages, iPadOS switches them on Control+Space; the pane
    /// must leave it to the system. With one, the pane gets C-Space.
    @Test func controlSpaceSwitchesTheLanguageWhenThereAreTwo() {
        #expect(KeyMap.map(special: nil, characters: " ", modifiers: [.control], inputModes: 2) == .none)
        #expect(KeyMap.map(special: nil, characters: " ", modifiers: [.control], inputModes: 1) == .keys(["C-Space"]))
        #expect(KeyMap.map(special: nil, characters: " ", modifiers: [.control, .option], inputModes: 2) == .keys(["C-M-Space"]))
    }

    @Test func plainCharactersAreText() {
        #expect(KeyMap.map(special: nil, characters: "a", modifiers: []) == .text("a"))
        #expect(KeyMap.map(special: nil, characters: "é", modifiers: [.shift]) == .text("é"))
    }

    @Test func commandAndUnnamedKeysStayInTheApp() {
        #expect(KeyMap.map(special: nil, characters: "k", modifiers: [.command]) == KeyMap.Output.none)
        #expect(KeyMap.map(special: .function(13), characters: "", modifiers: []) == KeyMap.Output.none)
        #expect(KeyMap.map(special: nil, characters: "é", modifiers: [.control]) == KeyMap.Output.none)
    }
}
