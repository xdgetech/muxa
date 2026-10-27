import Foundation
import MuxaProtocol
import MuxaTransport
import Testing

@testable import MuxaSession

struct ImageTreeTests {
    private let images = [
        "build/screenshots/en/login.png", "build/screenshots/en/home.png", "build/screenshots/ja/login.png",
        "docs/images/chat.jpg", "logo.png",
    ].map { ImageInfo(path: $0) }

    @Test func foldersNestAndSingleChildChainsJoin() {
        let root = ImageFolder.tree(images)
        #expect(root.images.map(\.path) == ["logo.png"])
        #expect(root.folders.map(\.name) == ["build/screenshots", "docs/images"])
        let shots = root.folders[0]
        #expect(shots.path == "build/screenshots")
        #expect(shots.folders.map(\.name) == ["en", "ja"])
        #expect(shots.folders[0].images.map(\.path) == ["build/screenshots/en/home.png", "build/screenshots/en/login.png"])
        #expect(shots.count == 3)
        #expect(root.count == 5)
    }

    @Test func siblingsAreTheFoldersImagesInNameOrder() {
        let next = ImageFolder.siblings(of: "build/screenshots/en/login.png", in: images)
        #expect(next.map(\.path) == ["build/screenshots/en/home.png", "build/screenshots/en/login.png"])
        #expect(ImageFolder.siblings(of: "logo.png", in: images).map(\.path) == ["logo.png"])
        #expect(ImageFolder.ancestors(of: "build/screenshots/en/login.png") == ["build", "build/screenshots", "build/screenshots/en"])
    }
}

@MainActor
struct ImageSessionTests {
    @Test func listsThenOpensAndFollowsAnImage() async throws {
        let bridge = FakeBridge()
        bridge.capabilities = ["images"]
        bridge.images = [ImageInfo(path: "shots/a.png", mtime: 2, size: 4)]
        let session = PaneSession()
        try await session.connect {
            bridge.start()
            return bridge
        }
        let listing = try await session.images(pane: "%1")
        #expect(listing.entries.map(\.path) == ["shots/a.png"])

        await session.openImage(pane: "%1", path: "shots/a.png")
        let ids = bridge.received.compactMap { if case .image(let id, _, _) = $0 { id } else { nil } }
        let id = try #require(ids.last)
        bridge.emit(.imageContent(image: id, path: "shots/a.png", data: Data([1, 2]), mtime: 2))
        try await waitUntil { session.image?.data == Data([1, 2]) }
        bridge.emit(.imageContent(image: id + 100, path: "shots/a.png", data: Data([9]), mtime: 3))
        bridge.emit(.imageEnded(image: id, reason: "removed"))
        try await waitUntil { session.image?.ended == "removed" }
        #expect(session.image?.data == Data([1, 2]))
    }
}
