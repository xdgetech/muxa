import Foundation
import MuxaProtocol
import MuxaTransport
import Testing

@testable import MuxaSession

@MainActor
struct HistoryTests {
    @Test func theHistoryComesBeforeItsResult() async throws {
        let bridge = FakeBridge()
        bridge.capabilities = ["history"]
        let session = PaneSession()
        try await session.connect {
            bridge.start()
            return bridge
        }
        #expect(session.canShowHistory)
        let history = try #require(try await session.history(pane: "%1"))
        #expect(history.pane == "%1")
        #expect(history.cols == 20)
        #expect(history.lineCount == 3)
        #expect(!history.truncated)
        #expect(try await session.history(pane: "%1", lines: 2)?.truncated == true)
    }

    @Test func eachNewlineAlsoReturnsForTheEmulator() {
        let history = PaneHistory(pane: "%1", cols: 4, data: Data("ab\ncd\n".utf8), truncated: false)
        #expect(history.terminalBytes == Data("ab\r\ncd\r\n".utf8))
        #expect(history.lineCount == 2)
    }
}
