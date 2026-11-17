#if DEBUG
import Foundation
import MuxaTransport
import Network

/// A real bridge behind a local TCP relay, for UI tests against a throwaway tmux server
/// (`make test-app-tmux`, launch argument `-bridge-port`). Never in a release build.
final class TCPBridge: BridgeChannel, @unchecked Sendable {
    let lines: AsyncThrowingStream<Data, any Error>
    private let connection: NWConnection
    private let queue = DispatchQueue(label: "muxa.tcp-bridge")

    /// Touched only on `queue`, which is serial.
    private final class State: @unchecked Sendable {
        var started = false
        var framer = LineFramer()
    }
    private let state = State()
    private let out: AsyncThrowingStream<Data, any Error>.Continuation

    init(port: UInt16) async throws {
        let connection = NWConnection(host: "127.0.0.1", port: NWEndpoint.Port(rawValue: port) ?? .any, using: .tcp)
        (lines, out) = AsyncThrowingStream<Data, any Error>.makeStream()
        self.connection = connection
        let state = state
        try await withCheckedThrowingContinuation { (ready: CheckedContinuation<Void, any Error>) in
            connection.stateUpdateHandler = { update in
                guard !state.started else { return }
                switch update {
                case .ready:
                    state.started = true
                    ready.resume()
                case .failed(let error), .waiting(let error):
                    state.started = true
                    ready.resume(throwing: TransportError.failed("\(error)"))
                default: break
                }
            }
            connection.start(queue: queue)
        }
        queue.async { [self] in receive() }
    }

    private func receive() {
        connection.receive(minimumIncompleteLength: 1, maximumLength: 1 << 16) { [self] data, _, done, error in
            if let data, !data.isEmpty {
                do {
                    for line in try state.framer.push(data) { out.yield(line) }
                } catch {
                    return out.finish(throwing: error)
                }
            }
            if let error { return out.finish(throwing: TransportError.failed("\(error)")) }
            if done { return out.finish() }
            receive()
        }
    }

    func send(_ line: Data) async throws {
        try await withCheckedThrowingContinuation { (sent: CheckedContinuation<Void, any Error>) in
            connection.send(content: line + Data([UInt8(ascii: "\n")]), completion: .contentProcessed { error in
                if error != nil { sent.resume(throwing: TransportError.closed) } else { sent.resume() }
            })
        }
    }

    func close() async {
        connection.cancel()
    }
}
#endif
