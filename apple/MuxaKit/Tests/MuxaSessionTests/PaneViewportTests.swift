import CoreGraphics
import MuxaProtocol
import Testing

@testable import MuxaSession

struct PaneViewportTests {
    @Test func aPhoneGetsASmallerDefaultFontThanATablet() {
        #expect(PaneViewport.fontSize(compact: true) == 10)
        #expect(PaneViewport.fontSize(compact: false) == 13)
        #expect(PaneViewport.fontSize(compact: true) >= PaneFit.minimumFontSize)
    }

    @Test func theGridIsTheWholeCellsThatFitWithOnePointOfSlack() {
        // A 13-inch iPad in landscape at 13 pt: 7.83 x 16 pt cells.
        let grid = PaneViewport.fitting(CGSize(width: 1376, height: 900), cell: CGSize(width: 7.83, height: 16))
        #expect(grid == PaneViewport(cols: 175, rows: 56))
        // Exactly cols x cells + 1 fits; one point less does not.
        #expect(PaneViewport.fitting(CGSize(width: 801, height: 241), cell: CGSize(width: 8, height: 16))
            == PaneViewport(cols: 100, rows: 15))
        #expect(PaneViewport.fitting(CGSize(width: 800, height: 240), cell: CGSize(width: 8, height: 16))
            == PaneViewport(cols: 99, rows: 14))
    }

    @Test func aPaneWithinTheViewportShowsWhole() {
        let viewport = PaneViewport(cols: 120, rows: 40)
        #expect(viewport.showsWhole(cols: 120, rows: 40))
        #expect(viewport.showsWhole(cols: 75, rows: 40))
        #expect(!viewport.showsWhole(cols: 100, rows: 60))
        #expect(!viewport.showsWhole(cols: 160, rows: 39))
    }

    @Test func theGridStaysWithinTheProtocolLimits() {
        let tiny = PaneViewport.fitting(CGSize(width: 40, height: 30), cell: CGSize(width: 6, height: 12))
        #expect(tiny == PaneViewport(cols: 10, rows: 5))
        let huge = PaneViewport.fitting(CGSize(width: 20000, height: 20000), cell: CGSize(width: 6, height: 12))
        #expect(huge == PaneViewport(cols: 1000, rows: 1000))
        #expect(PaneViewport.fitting(.zero, cell: CGSize(width: 6, height: 12)) == nil)
    }
}

@MainActor
struct PaneSessionViewportTests {
    func connected(_ capabilities: [String], viewport: PaneViewport? = nil) async throws -> (PaneSession, FakeBridge) {
        let bridge = FakeBridge()
        bridge.capabilities = capabilities
        let session = PaneSession()
        session.renderer = HeadlessScreen()
        if let viewport { session.setViewport(viewport) }
        try await session.connect {
            bridge.start()
            return bridge
        }
        return (session, bridge)
    }

    func viewports(_ bridge: FakeBridge) -> [PaneViewport] {
        bridge.received.compactMap {
            if case .viewport(_, let cols, let rows) = $0 { PaneViewport(cols: cols, rows: rows) } else { nil }
        }
    }

    @Test func theViewportGoesBeforeTheWatchOnConnect() async throws {
        let (session, bridge) = try await connected(["viewport"], viewport: PaneViewport(cols: 120, rows: 40))
        try await session.watch(pane: "%1")
        let kinds = bridge.received.map { req -> String in
            switch req {
            case .viewport: "viewport"
            case .watch: "watch"
            default: "other"
            }
        }.filter { $0 != "other" }
        #expect(kinds == ["viewport", "watch"])
        #expect(viewports(bridge) == [PaneViewport(cols: 120, rows: 40)])
    }

    @Test func aBridgeWithoutTheCapabilityGetsNoViewport() async throws {
        let (session, bridge) = try await connected([], viewport: PaneViewport(cols: 120, rows: 40))
        session.setViewport(PaneViewport(cols: 90, rows: 30))
        try await session.watch(pane: "%1")
        try await Task.sleep(for: .milliseconds(400))
        #expect(viewports(bridge).isEmpty)
    }

    @Test func changesAreSentOnceTheySettle() async throws {
        let (session, bridge) = try await connected(["viewport"], viewport: PaneViewport(cols: 120, rows: 40))
        // A rotation animates through several sizes.
        session.setViewport(PaneViewport(cols: 100, rows: 50))
        session.setViewport(PaneViewport(cols: 90, rows: 60))
        session.setViewport(PaneViewport(cols: 90, rows: 60))
        try await Task.sleep(for: .milliseconds(100))
        #expect(viewports(bridge) == [PaneViewport(cols: 120, rows: 40)])
        try await Task.sleep(for: .milliseconds(500))
        #expect(viewports(bridge) == [PaneViewport(cols: 120, rows: 40), PaneViewport(cols: 90, rows: 60)])
    }

    @Test func aReconnectSendsTheViewportAgain() async throws {
        let (session, first) = try await connected(["viewport"], viewport: PaneViewport(cols: 120, rows: 40))
        #expect(viewports(first).count == 1)
        let second = FakeBridge()
        second.capabilities = ["viewport"]
        try await session.connect {
            second.start()
            return second
        }
        #expect(viewports(second) == [PaneViewport(cols: 120, rows: 40)])
    }
}
