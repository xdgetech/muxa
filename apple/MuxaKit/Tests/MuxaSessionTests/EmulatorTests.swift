import Foundation
import Testing

@testable import MuxaSession

/// The two behaviors where vt100 (the Rust test oracle) differs from tmux.
/// See "Emulator differences" in docs/findings/tmux-control-mode.md.
@MainActor
struct EmulatorTests {
    func screen(_ bytes: String) -> HeadlessScreen {
        let s = HeadlessScreen()
        s.reset(cols: 20, rows: 10, data: Data(bytes.utf8))
        return s
    }

    @Test func decstbmHomesTheCursorToTheScreen() {
        let s = screen("\u{1b}[5;7H\u{1b}[3;8r")
        #expect(s.cursor == (0, 0))
    }

    @Test func theScrollRegionIsSharedByBothScreens() {
        // Region rows 3-5 set on the primary screen, then used on the alternate one.
        // With one region per terminal, the line feed at row 5 scrolls "A" out of row 3.
        // tmux gives ["", "", "B", "C"]. SwiftTerm keeps a region per buffer, as vt100 does.
        let s = screen("\u{1b}[3;5r\u{1b}[?1049h\u{1b}[3;1HA\u{1b}[4;1HB\u{1b}[5;1HC\n")
        withKnownIssue("SwiftTerm keeps one scroll region per screen") {
            #expect(s.rows == ["", "", "B", "C"])
        }
    }

    @Test func repliesToHostQueriesAreDropped() {
        // Device attributes and cursor position requests: tmux answers these itself.
        let s = screen("\u{1b}[c\u{1b}[6n\u{1b}[>c")
        #expect(s.rows.isEmpty)
    }
}
