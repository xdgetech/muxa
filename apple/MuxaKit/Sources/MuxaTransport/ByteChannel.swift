import Foundation

/// A running program as raw bytes both ways: protocol v2's transport.
/// Implementations: `SSHExec` (the app), `ProcessBytes` (macOS tests and tools).
public protocol ByteChannel: Sendable {
    /// Bytes as they arrive. Finishes when the program exits.
    var bytes: AsyncThrowingStream<Data, any Error> { get }
    func send(_ data: Data) async throws
    func close() async
}

public enum TransportError: Error, Equatable, Sendable {
    case closed
    /// The host presented a key that does not match the pinned one, or no key is pinned yet.
    case hostKeyMismatch(presented: HostKey)
    /// The host refused every login method offered: an unknown key or a wrong password.
    case notAuthorized
    case failed(String)
}
