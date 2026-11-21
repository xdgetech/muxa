import Testing

@testable import MuxaV2

struct KeyBytesTests {
    private func b(_ s: String) -> [UInt8] { Array(s.utf8) }

    @Test func namedKeysAreXtermSequences() {
        #expect(KeyBytes.bytes("Up") == b("\u{1b}[A"))
        #expect(KeyBytes.bytes("S-Up") == b("\u{1b}[1;2A"))
        #expect(KeyBytes.bytes("C-M-Left") == b("\u{1b}[1;7D"))
        #expect(KeyBytes.bytes("PageDown") == b("\u{1b}[6~"))
        #expect(KeyBytes.bytes("F1") == b("\u{1b}OP"))
        #expect(KeyBytes.bytes("F5") == b("\u{1b}[15~"))
        #expect(KeyBytes.bytes("Enter") == [13])
        #expect(KeyBytes.bytes("BSpace") == [0x7f])
        #expect(KeyBytes.bytes("BTab") == b("\u{1b}[Z"))
        #expect(KeyBytes.bytes("Escape") == [0x1b])
    }

    @Test func controlAndMetaCharacters() {
        #expect(KeyBytes.bytes("C-c") == [3])
        #expect(KeyBytes.bytes("C-Space") == [0])
        #expect(KeyBytes.bytes("C-[") == [0x1b])
        #expect(KeyBytes.bytes("M-x") == [0x1b, 0x78])
        #expect(KeyBytes.bytes("M-BSpace") == [0x1b, 0x7f])
        #expect(KeyBytes.bytes("M-Enter") == [0x1b, 13])
        // Shift is not Meta.
        #expect(KeyBytes.bytes("S-Enter") == [13])
        #expect(KeyBytes.bytes("S-BSpace") == [0x7f])
        #expect(KeyBytes.bytes("C-1") == nil)
        #expect(KeyBytes.bytes("Nonsense") == nil)
    }
}
