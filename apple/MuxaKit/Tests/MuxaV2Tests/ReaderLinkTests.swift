import Testing

@testable import MuxaV2

struct ReaderLinkTests {
    let entries = ["PLAN.md", ".muxa/TASK.md", "docs/a b.md", "docs/guide/setup.md"].map { ReaderEntry(path: $0) }

    @Test(arguments: [
        ("PLAN.md", ".muxa/TASK.md", ".muxa/TASK.md"),
        ("PLAN.md", "./.muxa/TASK.md#goal", ".muxa/TASK.md"),
        ("docs/guide/setup.md", "../../PLAN.md", "PLAN.md"),
        ("docs/guide/setup.md", "../a%20b.md?x=1", "docs/a b.md"),
        (".muxa/TASK.md", "../docs/guide/setup.md", "docs/guide/setup.md"),
    ])
    func aRelativeLinkNamesAListedDocument(from: String, href: String, path: String) {
        #expect(ReaderLink.document(href, from: from, in: entries) == path)
    }

    @Test(arguments: [
        "https://example.com/PLAN.md", "mailto:a@b.c", "javascript:alert(1)", "/PLAN.md", "//host/PLAN.md",
        "#top", "../PLAN.md", "NOTES.md", "",
    ])
    func anythingElseNamesNoDocument(href: String) {
        #expect(ReaderLink.document(href, from: "PLAN.md", in: entries) == nil)
    }
}
