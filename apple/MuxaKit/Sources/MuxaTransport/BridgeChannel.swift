import Foundation

/// A running bridge process: protocol lines in, protocol lines out.
/// Implementations: `SSHBridge` (the app), `ProcessBridge` (macOS tests and tools).
public protocol BridgeChannel: Sendable {
    /// Lines from the bridge, without the newline. Finishes when the bridge exits.
    var lines: AsyncThrowingStream<Data, any Error> { get }
    /// Writes one line; the newline is added here.
    func send(_ line: Data) async throws
    /// Closes the channel. The client sends `bye` first.
    func close() async
}

public enum TransportError: Error, Equatable, Sendable {
    /// The bridge wrote a line longer than `LineFramer.maxLine`.
    case lineTooLong
    case closed
    /// The host presented a key that does not match the pinned one, or no key is pinned yet.
    case hostKeyMismatch(presented: HostKey)
    case failed(String)
}

/// Splits a byte stream into lines. Bytes stay bytes until the JSON decoder sees them.
public struct LineFramer: Sendable {
    /// A `screen` of a large pane is about 1 MiB of base64. Far above that is a broken stream.
    public static let maxLine = 32 << 20

    private var pending = Data()

    public init() {}

    public mutating func push(_ bytes: Data) throws -> [Data] {
        pending.append(bytes)
        var lines: [Data] = []
        while let newline = pending.firstIndex(of: UInt8(ascii: "\n")) {
            lines.append(Data(pending[pending.startIndex..<newline]))
            pending.removeSubrange(pending.startIndex...newline)
        }
        if pending.count > Self.maxLine { throw TransportError.lineTooLong }
        return lines
    }
}
