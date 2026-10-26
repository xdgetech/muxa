import Testing

@testable import MuxaSession

struct CompositionTests {
    @Test func markedTextIsSentOnlyOnCommit() {
        var c = Composition()
        c.mark("に", selection: 1..<1)
        c.mark("にほんご", selection: 4..<4)
        #expect(c.isComposing)
        #expect(c.commit() == .text("にほんご"))
        #expect(!c.isComposing)
        #expect(c.commit() == .none)
    }

    @Test func convertedTextReplacesTheComposition() {
        var c = Composition()
        c.mark("にほんご", selection: 0..<4)
        #expect(c.insert("日本語") == .text("日本語"))
        #expect(!c.isComposing)
    }

    @Test func cancelledCompositionSendsNothing() {
        var c = Composition()
        c.mark("にほ", selection: 2..<2)
        c.mark("", selection: 0..<0)
        #expect(c.commit() == .none)
    }

    @Test func backspaceEditsTheCompositionFirst() {
        var c = Composition()
        c.mark("にほ", selection: 2..<2)
        #expect(c.deleteBackward() == .none)
        #expect(c.marked == "に")
        #expect(c.deleteBackward() == .none)
        #expect(c.deleteBackward() == .keys(["BSpace"]))
    }

    @Test func plainTypingPassesThrough() {
        var c = Composition()
        #expect(c.insert("a") == .text("a"))
        #expect(c.insert("\n") == .keys(["Enter"]))
    }

    @Test func selectionStaysInsideTheMarkedText() {
        var c = Composition()
        c.mark("ab", selection: 1..<9)
        #expect(c.selection == 1..<2)
    }
}
