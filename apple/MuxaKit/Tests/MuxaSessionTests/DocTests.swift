import Foundation
import MuxaProtocol
import MuxaTransport
import Testing

@testable import MuxaSession

@MainActor
struct DocSessionTests {
    func connected(_ bridge: FakeBridge) async throws -> PaneSession {
        let session = PaneSession()
        try await session.connect {
            bridge.start()
            return bridge
        }
        return session
    }

    private func docIds(_ bridge: FakeBridge) -> [UInt64] {
        bridge.received.compactMap { if case .doc(let id, _, _) = $0 { id } else { nil } }
    }

    @Test func listsThenOpensAndFollowsADocument() async throws {
        let bridge = FakeBridge()
        bridge.capabilities = ["docs"]
        bridge.docs = [DocInfo(path: "plans/p.md", kind: .plan, title: "Plan"), DocInfo(path: "README.md", kind: .repo)]
        let session = try await connected(bridge)
        let listing = try await session.documents(pane: "%1")
        #expect(listing.root == "app")
        #expect(listing.entries.map(\.path) == ["plans/p.md", "README.md"])

        await session.openDocument(pane: "%1", path: "README.md")
        let first = try #require(docIds(bridge).last)
        bridge.emit(.docContent(doc: first, path: "README.md", text: "# One", mtime: 1))
        try await waitUntil { session.doc?.text == "# One" }
        bridge.emit(.docContent(doc: first, path: "README.md", text: "# Two", mtime: 2))
        try await waitUntil { session.doc?.text == "# Two" }

        // Content for an earlier document is dropped once another is open.
        await session.openDocument(pane: "%1", path: "plans/p.md")
        let second = try #require(docIds(bridge).last)
        bridge.emit(.docContent(doc: first, path: "README.md", text: "stale", mtime: 3))
        bridge.emit(.docContent(doc: second, path: "plans/p.md", text: "# Plan", mtime: 3))
        try await waitUntil { session.doc?.text == "# Plan" }
        bridge.emit(.docEnded(doc: second, reason: "the file was removed"))
        try await waitUntil { session.doc?.ended == "the file was removed" }
    }

    @Test func aReconnectOpensTheSameDocumentAgain() async throws {
        let bridge = FakeBridge()
        bridge.capabilities = ["docs"]
        let session = try await connected(bridge)
        await session.openDocument(pane: "%1", path: "README.md")

        let next = FakeBridge()
        next.capabilities = ["docs"]
        try await session.connect {
            next.start()
            return next
        }
        #expect(docIds(next).count == 1)
        #expect(session.doc?.path == "README.md")
        #expect(session.doc?.text == nil)
    }

    @Test func nothingIsRequestedWithoutTheCapability() async throws {
        let bridge = FakeBridge()
        let session = try await connected(bridge)
        await session.openDocument(pane: "%1", path: "README.md")
        #expect(session.doc == nil)
        #expect(docIds(bridge).isEmpty)
    }
}

@MainActor
struct DiffSessionTests {
    private func diffIds(_ bridge: FakeBridge) -> [UInt64] {
        bridge.received.compactMap { if case .diff(let id, _, _, _) = $0 { id } else { nil } }
    }

    @Test func aBranchDiffSaysItsScopeAndAnUncommittedOneSaysNothing() async throws {
        let bridge = FakeBridge()
        bridge.capabilities = ["changes", "branch-changes"]
        let session = PaneSession()
        try await session.connect {
            bridge.start()
            return bridge
        }
        await session.openDiff(pane: "%1", path: "a.rs", scope: .branch)
        await session.openDiff(pane: "%1", path: "a.rs")
        let scopes = bridge.received.compactMap { if case .diff(_, _, _, let scope) = $0 { Optional(scope) } else { nil } }
        #expect(scopes == [.branch, nil])
        #expect(session.diff?.scope == .uncommitted)
    }

    @Test func listsChangesThenFollowsADiff() async throws {
        let bridge = FakeBridge()
        bridge.capabilities = ["changes"]
        bridge.changes = [ChangeInfo(path: "a.rs", status: .modified, added: 1, removed: 1), ChangeInfo(path: "logo.png", status: .added)]
        let session = PaneSession()
        try await session.connect {
            bridge.start()
            return bridge
        }
        let listing = try await session.changes(pane: "%1")
        #expect(listing.entries.map(\.path) == ["a.rs", "logo.png"])

        await session.openDiff(pane: "%1", path: "a.rs")
        let first = try #require(diffIds(bridge).last)
        #expect(session.diff?.loaded == false)
        bridge.emit(.diffContent(diff: first, path: "a.rs", text: "@@ -1 +1 @@\n-a\n+b\n", binary: false))
        try await waitUntil { session.diff?.text?.contains("+b") == true }

        // A diff for an earlier request is dropped once another is open.
        await session.openDiff(pane: "%1", path: "logo.png")
        let second = try #require(diffIds(bridge).last)
        bridge.emit(.diffContent(diff: first, path: "a.rs", text: "stale", binary: false))
        bridge.emit(.diffContent(diff: second, path: "logo.png", text: nil, binary: true))
        try await waitUntil { session.diff?.binary == true }
        #expect(session.diff?.text == nil)
        bridge.emit(.diffEnded(diff: second, reason: "logo.png has no changes now"))
        try await waitUntil { session.diff?.ended == "logo.png has no changes now" }

        await session.closeDiff()
        #expect(session.diff == nil)
    }
}
