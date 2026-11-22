import Foundation
import MuxaSession
import Testing

@Suite struct KeyChordTests {
    @Test func printsInTheAppsOrder() {
        #expect(KeyChord("h", [.command, .shift]).display == "⌘⇧H")
        #expect(KeyChord(.space, [.control, .option]).display == "⌃⌥Space")
        #expect(KeyChord(.f8).display == "F8")
        #expect(KeyChord("[", .command).display == "⌘[")
    }

    @Test func anUppercaseLetterIsStoredLowercase() {
        #expect(KeyChord("D", .command) == KeyChord("d", .command))
    }

    @Test func survivesJSON() throws {
        let chords = [KeyChord("w", .control), KeyChord(.left, [.option, .command]), KeyChord("\\", .command)]
        let data = try JSONEncoder().encode(chords)
        #expect(try JSONDecoder().decode([KeyChord].self, from: data) == chords)
    }

    @Test func refusesMoreThanOneCharacter() {
        let data = Data(#"{"key":{"character":"ab"},"modifiers":4}"#.utf8)
        #expect(throws: DecodingError.self) { try JSONDecoder().decode(KeyChord.self, from: data) }
    }
}

@Suite struct WindowKeysTests {
    let prefix = KeyChord("w", .control)

    private func after(_ chord: KeyChord) -> WindowKeys.Outcome {
        var keys = WindowKeys()
        #expect(keys.press(prefix, prefix: prefix) == .consume)
        #expect(keys.pending)
        let outcome = keys.press(chord, prefix: prefix)
        #expect(!keys.pending)
        return outcome
    }

    @Test func otherKeysGoToTheTerminal() {
        var keys = WindowKeys()
        #expect(keys.press(KeyChord("l"), prefix: prefix) == .pass)
        #expect(keys.press(KeyChord("w", .command), prefix: prefix) == .pass)
        #expect(!keys.pending)
    }

    @Test func vimsWindowCommands() {
        #expect(after(KeyChord("v")) == .run(.split))
        #expect(after(KeyChord("s")) == .run(.split))
        #expect(after(KeyChord("v", .control)) == .run(.split))
        #expect(after(KeyChord("l")) == .run(.next))
        #expect(after(KeyChord(.right)) == .run(.next))
        #expect(after(KeyChord("w")) == .run(.next))
        #expect(after(KeyChord("h")) == .run(.previous))
        #expect(after(KeyChord("w", .shift)) == .run(.previous))
        #expect(after(KeyChord("q")) == .run(.close))
        #expect(after(KeyChord("c")) == .run(.close))
        #expect(after(KeyChord("o")) == .run(.closeOthers))
        #expect(after(KeyChord("3")) == .run(.pane(3)))
    }

    @Test func thePrefixTwiceReachesTheTerminal() {
        #expect(after(prefix) == .pass)
    }

    @Test func anUnknownKeyCancelsWithoutTyping() {
        #expect(after(KeyChord("j")) == .consume)
        #expect(after(KeyChord(.escape)) == .consume)
        #expect(after(KeyChord("0")) == .consume)
    }

    @Test func cancelClearsThePrefix() {
        var keys = WindowKeys()
        _ = keys.press(prefix, prefix: prefix)
        keys.cancel()
        #expect(keys.press(KeyChord("v"), prefix: prefix) == .pass)
    }
}
