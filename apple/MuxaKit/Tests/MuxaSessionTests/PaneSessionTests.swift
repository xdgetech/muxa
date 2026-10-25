import Foundation
import MuxaProtocol
import MuxaTransport
import Testing

@testable import MuxaSession

/// An in-memory bridge: answers every request, and records them in order.
class FakeBridge: BridgeChannel, @unchecked Sendable {
    let lines: AsyncThrowingStream<Data, any Error>
    private let out: AsyncThrowingStream<Data, any Error>.Continuation
    private let lock = NSLock()
    private var _received: [Request] = []
    private var _closed = false
    var protocolVersion: UInt32 = 1
    var sessions: [Session] = [
        Session(id: "$1", name: "demo", panes: [
            Pane(id: "%1", window: "@1", index: 0, role: .agent, cols: 20, rows: 5,
                 command: "claude", path: "/w", dead: false, shellForeground: false),
        ])
    ]
    var capabilities: [String] = []
    var docs: [DocInfo] = []
    /// Panes whose `chat` is answered with `unsupported`.
    var unsupportedChats: Set<String> = []
    /// Screen data sent after each successful watch.
    var screen = Data("\u{1b}chello".utf8)

    init() {
        (lines, out) = AsyncThrowingStream<Data, any Error>.makeStream()
    }

    var received: [Request] { lock.withLock { _received } }
    var closed: Bool { lock.withLock { _closed } }

    func start() { emit(.hello(protocol: protocolVersion, bridge: "test", tmux: "3.7c", capabilities: capabilities)) }

    func emit(_ message: Message) { out.yield(try! Wire.encode(message)) }

    func send(_ line: Data) async throws {
        let req = try Wire.decodeRequest(line)
        lock.withLock { _received.append(req) }
        switch req {
        case .list(let id): emit(.result(id: id, error: nil, sessions: sessions))
        case .watch(let id, let pane):
            emit(.result(id: id, error: nil, sessions: nil))
            emit(.screen(watch: id, pane: pane, cols: 20, rows: 5, data: screen))
        case .docs(let id, _):
            emit(.docList(DocListing(docs: id, root: "app", entries: docs)))
            emit(.result(id: id, error: nil, sessions: nil))
        case .chat(let id, let pane) where unsupportedChats.contains(pane):
            emit(.result(id: id, error: ErrorBody(code: .unsupported, message: "no adapter"), sessions: nil))
        case .bye: out.finish()
        default: emit(.result(id: req.id, error: nil, sessions: nil))
        }
    }

    func close() async {
        lock.withLock { _closed = true }
        out.finish()
    }
}

@MainActor
struct PaneSessionTests {
    func connected(_ bridge: FakeBridge = FakeBridge()) async throws -> (PaneSession, HeadlessScreen, FakeBridge) {
        let session = PaneSession()
        let screen = HeadlessScreen()
        session.renderer = screen
        try await session.connect {
            bridge.start()
            return bridge
        }
        return (session, screen, bridge)
    }

    @Test func connectReadsHelloAndLists() async throws {
        let (session, _, _) = try await connected()
        #expect(session.phase == .connected)
        #expect(session.tmuxVersion == "3.7c")
        #expect(session.sessions.map(\.name) == ["demo"])
    }

    @Test func aProtocolMismatchFailsTheConnect() async throws {
        let bridge = FakeBridge()
        bridge.protocolVersion = 2
        let session = PaneSession()
        await #expect(throws: PaneSession.Failure.protocolMismatch(bridge: 2)) {
            try await session.connect {
                bridge.start()
                return bridge
            }
        }
        guard case .disconnected = session.phase else {
            Issue.record("expected disconnected, got \(session.phase)")
            return
        }
        #expect(bridge.closed)
    }

    @Test func watchRendersTheScreenAndDropsStaleWatches() async throws {
        let (session, screen, bridge) = try await connected()
        try await session.watch(pane: "%1")
        try await waitUntil { screen.rows == ["hello"] }

        // Output for an older watch must not reach the renderer.
        bridge.emit(.output(watch: 999, pane: "%1", data: Data(" stale".utf8)))
        guard case .watch(let watch, _) = bridge.received.last else {
            Issue.record("no watch sent")
            return
        }
        bridge.emit(.output(watch: watch, pane: "%1", data: Data(" world".utf8)))
        try await waitUntil { screen.rows == ["hello world"] }
    }

    @Test func inputKeepsItsOrder() async throws {
        let (session, _, bridge) = try await connected()
        try await session.watch(pane: "%1")
        session.send(text: "a")
        session.send(keys: ["Enter"])
        session.send(bytes: Data([0x1b]))
        session.paste("p")
        try await waitUntil { bridge.received.count == 6 }
        let tail = bridge.received.suffix(4).map { req -> String in
            switch req {
            case .text(_, "%1", let t): "text \(t)"
            case .keys(_, "%1", let k): "keys \(k.joined())"
            case .bytes(_, "%1", let d): "bytes \(d.count)"
            case .paste(_, "%1", let t): "paste \(t)"
            default: "other"
            }
        }
        #expect(tail == ["text a", "keys Enter", "bytes 1", "paste p"])
    }

    @Test func paneGoneIsReported() async throws {
        let (session, _, bridge) = try await connected()
        try await session.watch(pane: "%1")
        guard case .watch(let watch, _) = bridge.received.last else { return }
        bridge.emit(.paneGone(watch: watch, pane: "%1", reason: nil))
        try await waitUntil { session.paneGoneReason != nil }
    }

    @Test func byeFromTheBridgeDisconnects() async throws {
        let (session, _, bridge) = try await connected()
        bridge.emit(.bye(reason: "idle timeout"))
        try await waitUntil { session.phase == .disconnected(reason: "idle timeout") }
    }

    @Test func disconnectSendsBye() async throws {
        let (session, _, bridge) = try await connected()
        await session.disconnect()
        try await waitUntil {
            if case .bye = bridge.received.last { return true }
            return false
        }
        #expect(bridge.closed)
    }

    @Test func reconnectWatchesTheSamePane() async throws {
        let (session, screen, first) = try await connected()
        try await session.watch(pane: "%1")
        await first.close()
        try await waitUntil {
            if case .disconnected = session.phase { return true }
            return false
        }

        let second = FakeBridge()
        second.screen = Data("\u{1b}cagain".utf8)
        try await session.connect {
            second.start()
            return second
        }
        try await waitUntil { screen.rows == ["again"] }
        #expect(session.watchedPane == "%1")
    }

    @Test func aFailedInputIsShown() async throws {
        let bridge = FailingInput()
        let (session, _, _) = try await connected(bridge)
        try await session.watch(pane: "%1")
        session.send(keys: ["Nope"])
        try await waitUntil { session.lastError == "unsupported key Nope" }
    }
}

/// Rejects every `keys` request.
final class FailingInput: FakeBridge, @unchecked Sendable {
    override func send(_ line: Data) async throws {
        if case .keys(let id, _, let keys) = try Wire.decodeRequest(line) {
            emit(.result(id: id, error: ErrorBody(code: .unsupportedKey, message: "unsupported key \(keys[0])"), sessions: nil))
            return
        }
        try await super.send(line)
    }
}

@MainActor
func waitUntil(_ condition: () -> Bool, timeout: Duration = .seconds(5)) async throws {
    let deadline = ContinuousClock.now + timeout
    while !condition() {
        if ContinuousClock.now > deadline {
            Issue.record("condition not met in \(timeout)")
            return
        }
        try await Task.sleep(for: .milliseconds(10))
    }
}
