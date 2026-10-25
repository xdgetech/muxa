@preconcurrency import Citadel
import Foundation
import NIOCore
import NIOSSH

/// Where and how to reach the bridge.
public struct SSHTarget: Codable, Hashable, Sendable {
    public var host: String
    public var port: Int
    public var user: String
    /// The exec command. With a restricted `authorized_keys` entry the server ignores it.
    public var command: String
    /// The host key accepted on first use. Nil until the user has confirmed one.
    public var hostKey: HostKey?

    public init(host: String, port: Int = 22, user: String, command: String = "~/.local/bin/muxa-bridge", hostKey: HostKey? = nil) {
        self.host = host
        self.port = port
        self.user = user
        self.command = command
        self.hostKey = hostKey
    }
}

/// The bridge as an SSH exec channel. No PTY: bytes pass through unchanged.
public final class SSHBridge: BridgeChannel {
    public let lines: AsyncThrowingStream<Data, any Error>
    private let writer: Writer
    private let task: Task<Void, Never>

    /// Connects and starts the bridge. Fails with `hostKeyMismatch` when the host key
    /// is not the pinned one, or when none is pinned yet: the caller shows its
    /// fingerprint, and connects again with it pinned once the user accepts it.
    public static func connect(to target: SSHTarget, key: ClientKey) async throws -> SSHBridge {
        let check = HostKeyCheck(pinned: target.hostKey)
        let client: SSHClient
        do {
            client = try await SSHClient.connect(
                host: target.host,
                port: target.port,
                authenticationMethod: .ed25519(username: target.user, privateKey: key.key),
                hostKeyValidator: .custom(check),
                reconnect: .never,
                connectTimeout: .seconds(15))
        } catch {
            if let presented = check.rejected { throw TransportError.hostKeyMismatch(presented: presented) }
            throw TransportError.failed("cannot connect to \(target.host): \(error)")
        }
        return try await SSHBridge(client: client, command: target.command)
    }

    private init(client: SSHClient, command: String) async throws {
        let (lines, linesOut) = AsyncThrowingStream<Data, any Error>.makeStream()
        let writer = Writer(client: client)
        self.lines = lines
        self.writer = writer
        let (ready, readyOut) = AsyncStream<Void>.makeStream()
        task = Task {
            do {
                try await client.withExec(command) { inbound, outbound in
                    await writer.attach(outbound)
                    readyOut.yield()
                    var framer = LineFramer()
                    for try await chunk in inbound {
                        // stderr carries the bridge's own log (no pane bytes); not protocol.
                        guard case .stdout(let buffer) = chunk else { continue }
                        for line in try framer.push(Data(buffer.readableBytesView)) {
                            linesOut.yield(line)
                        }
                    }
                }
                linesOut.finish()
            } catch {
                linesOut.finish(throwing: error)
            }
            readyOut.finish()
            await writer.detach()
        }
        for await _ in ready { break }
    }

    public func send(_ line: Data) async throws {
        var buffer = ByteBuffer()
        buffer.writeBytes(line)
        buffer.writeInteger(UInt8(ascii: "\n"))
        try await writer.write(buffer)
    }

    public func close() async {
        task.cancel()
        await writer.close()
    }
}

/// Owns the SSH client and the exec channel's stdin writer, which lives inside `withExec`.
private actor Writer {
    private let client: SSHClient
    private var outbound: TTYStdinWriter?

    init(client: SSHClient) { self.client = client }

    func close() async {
        outbound = nil
        try? await client.close()
    }

    func attach(_ w: TTYStdinWriter) { outbound = w }
    func detach() { outbound = nil }

    func write(_ buffer: ByteBuffer) async throws {
        guard let outbound else { throw TransportError.closed }
        try await outbound.write(buffer)
    }
}

/// Accepts only the pinned host key, and remembers a rejected one for the caller.
private final class HostKeyCheck: NIOSSHClientServerAuthenticationDelegate, @unchecked Sendable {
    private let pinned: HostKey?
    private let lock = NSLock()
    private var _rejected: HostKey?

    init(pinned: HostKey?) { self.pinned = pinned }

    var rejected: HostKey? { lock.withLock { _rejected } }

    func validateHostKey(hostKey: NIOSSHPublicKey, validationCompletePromise: EventLoopPromise<Void>) {
        let presented = HostKey(hostKey)
        if presented == pinned {
            validationCompletePromise.succeed(())
        } else {
            lock.withLock { _rejected = presented }
            validationCompletePromise.fail(TransportError.hostKeyMismatch(presented: presented))
        }
    }
}
