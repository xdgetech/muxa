import Foundation
import MuxaProtocol
import MuxaTransport
import Observation

/// One connection to a bridge: hello, the session list, and the watched pane.
/// The UI reads its state; the renderer gets the pane's bytes.
@MainActor
@Observable
public final class PaneSession {
    public enum Phase: Equatable, Sendable {
        case idle
        case connecting
        case connected
        case disconnected(reason: String)
    }

    public enum Failure: Error, Equatable {
        case notConnected
        case bridge(ErrorBody)
        case protocolMismatch(bridge: UInt32)
        case noHello
    }

    public typealias Opener = @Sendable () async throws -> any BridgeChannel

    public private(set) var phase: Phase = .idle
    public private(set) var tmuxVersion: String?
    public private(set) var sessions: [Session] = []
    /// The pane shown, and kept across reconnects.
    public private(set) var watchedPane: String?
    /// Set when the watched pane disappears; cleared by the next watch.
    public private(set) var paneGoneReason: String?
    /// The last failed input or command, for a short notice in the UI.
    public private(set) var lastError: String?

    @ObservationIgnored public weak var renderer: (any PaneRenderer)?

    @ObservationIgnored private var channel: (any BridgeChannel)?
    @ObservationIgnored private var outbox: AsyncStream<Data>.Continuation?
    @ObservationIgnored private var writer: Task<Void, Never>?
    @ObservationIgnored private var tasks: [Task<Void, Never>] = []
    @ObservationIgnored private var nextId: UInt64 = 0
    @ObservationIgnored private var watchId: UInt64?
    @ObservationIgnored private var pending: [UInt64: CheckedContinuation<Message, any Error>] = [:]
    /// Ids of input requests in flight; their results only matter when they fail.
    @ObservationIgnored private var inputs: Set<UInt64> = []
    @ObservationIgnored private var hello: CheckedContinuation<Void, any Error>?
    @ObservationIgnored private let pingEvery: Duration

    public init(pingEvery: Duration = .seconds(15)) {
        self.pingEvery = pingEvery
    }

    // MARK: Lifecycle

    /// Opens a channel, waits for `hello`, and lists sessions. On a reconnect it
    /// watches the same pane again, unless `samePanes` is false (another host).
    public func connect(_ open: Opener, samePanes: Bool = true) async throws {
        await teardown(reason: nil)
        if !samePanes {
            watchedPane = nil
            paneGoneReason = nil
            sessions = []
        }
        phase = .connecting
        do {
            let channel = try await open()
            start(channel)
            try await waitForHello()
            phase = .connected
            try await refresh()
            if let pane = watchedPane { try await watch(pane: pane) }
        } catch {
            await teardown(reason: Self.describe(error))
            throw error
        }
    }

    /// Sends `bye` and closes the channel.
    public func disconnect() async {
        if channel != nil { enqueue(.bye(id: takeId())) }
        await teardown(reason: "disconnected")
    }

    // MARK: Requests

    public func refresh() async throws {
        let reply = try await request { .list(id: $0) }
        if case .result(_, _, let sessions?) = reply { self.sessions = sessions }
    }

    /// Shows this pane. Messages for any earlier watch are dropped from now on.
    public func watch(pane: String) async throws {
        watchedPane = pane
        paneGoneReason = nil
        try await request { id in
            self.watchId = id
            return .watch(id: id, pane: pane)
        }
    }

    public func send(keys: [String]) { fire { .keys(id: $0, pane: $1, keys: keys) } }
    public func send(text: String) { fire { .text(id: $0, pane: $1, text: text) } }
    public func send(bytes: Data) { fire { .bytes(id: $0, pane: $1, data: bytes) } }
    public func paste(_ text: String) { fire { .paste(id: $0, pane: $1, text: text) } }

    /// Input for the watched pane, queued synchronously so it keeps call order.
    /// Its result is not awaited; a failure lands in `lastError`.
    private func fire(_ make: (UInt64, String) -> Request) {
        guard let pane = watchedPane, channel != nil else { return }
        let req = make(takeId(), pane)
        inputs.insert(req.id)
        enqueue(req)
    }

    @discardableResult
    private func request(_ make: (UInt64) -> Request) async throws -> Message {
        guard channel != nil else { throw Failure.notConnected }
        let req = make(takeId())
        // Registered before the line is queued, so the result cannot arrive first.
        return try await withCheckedThrowingContinuation { cont in
            pending[req.id] = cont
            enqueue(req)
        }
    }

    // MARK: Plumbing

    private func takeId() -> UInt64 {
        nextId += 1
        return nextId
    }

    private func enqueue(_ req: Request) {
        guard let line = try? Wire.encode(req) else { return }
        outbox?.yield(line)
    }

    private func start(_ channel: any BridgeChannel) {
        self.channel = channel
        let (lines, outbox) = AsyncStream<Data>.makeStream()
        self.outbox = outbox
        let pingEvery = self.pingEvery
        // One writer keeps requests in the order they were made.
        writer = Task {
            for await line in lines {
                do { try await channel.send(line) } catch { break }
            }
        }
        tasks = [
            Task { [weak self] in
                do {
                    for try await line in channel.lines {
                        self?.receive(line)
                    }
                    await self?.teardown(reason: "connection closed")
                } catch {
                    await self?.teardown(reason: Self.describe(error))
                }
            },
            Task { [weak self] in
                while !Task.isCancelled {
                    try? await Task.sleep(for: pingEvery)
                    guard let self, self.channel != nil else { return }
                    self.enqueue(.ping(id: self.takeId()))
                }
            },
        ]
    }

    private func waitForHello() async throws {
        let timeout = Task { [weak self] in
            try? await Task.sleep(for: .seconds(10))
            guard !Task.isCancelled, let self else { return }
            self.hello?.resume(throwing: Failure.noHello)
            self.hello = nil
        }
        defer { timeout.cancel() }
        // Set before the reader can run: both are on the main actor.
        try await withCheckedThrowingContinuation { hello = $0 }
    }

    private func receive(_ line: Data) {
        guard let message = try? Wire.decodeMessage(line) else { return }
        switch message {
        case .hello(let version, _, let tmux, _):
            if version == protocolVersion {
                tmuxVersion = tmux
                hello?.resume()
            } else {
                hello?.resume(throwing: Failure.protocolMismatch(bridge: version))
            }
            hello = nil
        case .result(let id?, let error, _) where inputs.contains(id):
            inputs.remove(id)
            if let error { lastError = error.message }
        case .result(let id?, let error, _):
            guard let cont = pending.removeValue(forKey: id) else { return }
            if let error { cont.resume(throwing: Failure.bridge(error)) } else { cont.resume(returning: message) }
        case .result(nil, let error, _):
            lastError = error?.message
        case .screen(let watch, _, let cols, let rows, let data) where watch == watchId:
            renderer?.reset(cols: Int(cols), rows: Int(rows), data: data)
        case .output(let watch, _, let data) where watch == watchId:
            renderer?.feed(data)
        case .paneGone(let watch, _, let reason) where watch == watchId:
            paneGoneReason = reason ?? "the pane closed"
        case .sessionsChanged:
            Task { try? await refresh() }
        case .bye(let reason):
            Task { await teardown(reason: reason) }
        case .screen, .output, .paneGone, .chatEntries, .chatEnded, .unknown:
            break
        }
    }

    private func teardown(reason: String?) async {
        guard let channel else {
            // Keep the first reason: a failed connect tears down twice.
            if let reason, phase == .connecting { phase = .disconnected(reason: reason) }
            return
        }
        self.channel = nil
        phase = .disconnected(reason: reason ?? "disconnected")
        tasks.forEach { $0.cancel() }
        tasks = []
        // Let queued lines (a final `bye`) go out, but do not wait on a dead link.
        outbox?.finish()
        outbox = nil
        if let writer {
            let drained = Task { await writer.value }
            let timeout = Task {
                try? await Task.sleep(for: .seconds(1))
                writer.cancel()
            }
            await drained.value
            timeout.cancel()
        }
        writer = nil
        watchId = nil
        inputs = []
        let failed = pending
        pending = [:]
        failed.values.forEach { $0.resume(throwing: Failure.notConnected) }
        hello?.resume(throwing: Failure.notConnected)
        hello = nil
        await channel.close()
    }

    /// A message for the user, saying what to do next where there is something to do.
    public nonisolated static func describe(_ error: any Error) -> String {
        switch error {
        case Failure.bridge(let body): body.message
        case Failure.protocolMismatch(let v): "the bridge speaks protocol \(v); this app speaks \(protocolVersion). Update one of them."
        case Failure.noHello: "the bridge did not start. Check that muxa-bridge is installed on the host."
        case Failure.notConnected: "not connected"
        case TransportError.hostKeyMismatch: "the host key changed or is not trusted yet"
        case TransportError.notAuthorized: "the host does not accept this iPad's key yet. Set it up with your password."
        case TransportError.failed(let message): message
        default: String(describing: error)
        }
    }
}
