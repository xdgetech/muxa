import Testing

@testable import MuxaSession

struct KeyBarTests {
    @Test func keysAreNamedForTmux() {
        var bar = KeyBar()
        let cases: [(KeyBar.Key, String)] = [
            (.escape, "Escape"), (.tab, "Tab"), (.backTab, "BTab"), (.interrupt, "C-c"),
            (.left, "Left"), (.down, "Down"), (.up, "Up"), (.right, "Right"),
        ]
        for (key, name) in cases {
            #expect(bar.tap(key) == .keys([name]))
        }
    }

    @Test func controlAppliesToTheNextCharacterOnly() {
        var bar = KeyBar()
        #expect(bar.tap(.control) == .none)
        #expect(bar.control)
        #expect(bar.type(.text("c")) == .keys(["C-c"]))
        #expect(!bar.control)
        #expect(bar.type(.text("c")) == .text("c"))
    }

    @Test func controlAppliesToTheNextBarKey() {
        var bar = KeyBar()
        _ = bar.tap(.control)
        #expect(bar.tap(.left) == .keys(["C-Left"]))
        #expect(bar.tap(.left) == .keys(["Left"]))
    }

    @Test func controlCanBeReleasedByTappingItAgain() {
        var bar = KeyBar()
        _ = bar.tap(.control)
        _ = bar.tap(.control)
        #expect(bar.type(.text("c")) == .text("c"))
    }

    @Test func textControlCannotNameIsSentAsTyped() {
        var bar = KeyBar()
        _ = bar.tap(.control)
        #expect(bar.type(.text("日本")) == .text("日本"))
        #expect(!bar.control)
    }

    @Test func aCompositionEditKeepsControlPending() {
        var bar = KeyBar()
        _ = bar.tap(.control)
        #expect(bar.type(.none) == .none)
        #expect(bar.control)
        #expect(bar.type(.keys(["Enter"])) == .keys(["Enter"]))
        #expect(!bar.control)
    }

    @Test func peekLeavesControlPending() {
        var bar = KeyBar()
        _ = bar.tap(.control)
        #expect(bar.peek(.text("c")) == .keys(["C-c"]))
        #expect(bar.control)
    }
}
