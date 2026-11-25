import Testing

@testable import MuxaSession

struct TerminalLinkTests {
    @Test func webAndMailLinksOpen() {
        #expect(TerminalLink.url("https://example.com/path?q=1")?.absoluteString == "https://example.com/path?q=1")
        #expect(TerminalLink.url("HTTP://example.com")?.host == "example.com")
        #expect(TerminalLink.url(" https://example.com \n")?.absoluteString == "https://example.com")
        #expect(TerminalLink.url("mailto:someone@example.com") != nil)
    }

    /// The host's output may name any scheme or path; none of those opens here.
    @Test func otherLinksDoNotOpen() {
        for link in ["file:///Applications/Calculator.app", "/etc/hosts", "~/notes.md", "src/main.rs:12",
                     "ssh://host", "javascript:alert(1)", "x-apple.systempreferences:", "https://", "https:path", ""] {
            #expect(TerminalLink.url(link) == nil, "\(link)")
        }
    }
}
