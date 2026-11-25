import Testing

@testable import MuxaV2

struct ReaderFolderTests {
    private func folders(_ paths: [String]) -> [ReaderFolder] {
        ReaderFolder.group(paths.map { ReaderEntry(path: $0) })
    }

    @Test func filesGoUnderTheirParentFolderInTheListsOrder() {
        let got = folders(["docs/b.md", "PLAN.md", "docs/adr/1.md", "docs/a.md", "README.md"])
        #expect(got.map(\.path) == ["docs", "", "docs/adr"])
        #expect(got.map { $0.entries.map(\.path) } == [["docs/b.md", "docs/a.md"], ["PLAN.md", "README.md"], ["docs/adr/1.md"]])
    }

    @Test func onlyAFolderWithSeveralFilesCollapses() {
        let got = folders(["shots/a.png", "shots/b.png", ".muxa/TASK.md", "PLAN.md", "NOTES.md"])
        #expect(got.map(\.collapses) == [true, false, false])
    }

    @Test func aFilesNameDropsItsFolder() {
        #expect(ReaderFolder.name(of: "docs/adr/0001-x.md") == "0001-x.md")
        #expect(ReaderFolder.name(of: "PLAN.md") == "PLAN.md")
    }

    @Test func aFilesAncestorsGoUpToTheListedFolder() {
        #expect(ReaderFolder.ancestors(of: "output/qa/T-1/run/report.md") == ["output/qa/T-1/run", "output/qa/T-1", "output/qa", "output"])
        #expect(ReaderFolder.ancestors(of: "PLAN.md").isEmpty)
    }

    @Test func aFoldersZipIsNamedAfterIt() {
        #expect(ReaderFolder.zipName("output/qa/run", root: "app") == "run.zip")
        #expect(ReaderFolder.zipName("", root: "app") == "app.zip")
        #expect(ReaderFolder.zipName("", root: nil) == "Folder.zip")
    }

    @Test func anEmptyListHasNoFolders() {
        #expect(folders([]).isEmpty)
    }
}
