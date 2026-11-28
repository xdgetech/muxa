import Testing

@testable import MuxaSession

struct ChatReturnTests {
    @Test func aTypedReturnSends() {
        #expect(ChatReturn.typed(old: "hello", new: "hello\n", imeOwnsReturn: false) == .send("hello"))
        #expect(ChatReturn.typed(old: "ab", new: "a\nb", imeOwnsReturn: false) == .send("ab"))
    }

    @Test func aReturnTheInputMethodOwnsOnlyConfirms() {
        #expect(ChatReturn.typed(old: "漢字", new: "漢字\n", imeOwnsReturn: true) == .confirm("漢字"))
    }

    @Test func otherEditsAreNoReturn() {
        #expect(ChatReturn.typed(old: "かんじ", new: "漢字", imeOwnsReturn: true) == nil)
        #expect(ChatReturn.typed(old: "", new: "a\nb", imeOwnsReturn: false) == nil)
        #expect(ChatReturn.typed(old: "ab", new: "abc", imeOwnsReturn: false) == nil)
        #expect(ChatReturn.typed(old: "a\n", new: "a", imeOwnsReturn: false) == nil)
    }

    @Test func theInputMethodOwnsReturnWhileComposing() {
        var ime = IMEReturn()
        ime.changed(marked: true, at: 10)
        #expect(ime.ownsReturn(marked: true, at: 20))
    }

    @Test func theConfirmsOwnKeyPressDoesNotSend() {
        var ime = IMEReturn()
        ime.changed(marked: true, at: 10)
        // The confirm, then the new line of the same key press.
        ime.changed(marked: false, at: 12)
        ime.changed(marked: false, at: 12.01)
        #expect(ime.ownsReturn(marked: false, at: 12.02))
    }

    @Test func aReturnAfterTheConfirmSends() {
        var ime = IMEReturn()
        ime.changed(marked: true, at: 10)
        ime.changed(marked: false, at: 12)
        #expect(!ime.ownsReturn(marked: false, at: 12.5))
    }

    @Test func plainTypingNeverHandsReturnToTheInputMethod() {
        var ime = IMEReturn()
        ime.changed(marked: false, at: 10)
        ime.changed(marked: false, at: 10.01)
        #expect(!ime.ownsReturn(marked: false, at: 10.02))
    }
}
