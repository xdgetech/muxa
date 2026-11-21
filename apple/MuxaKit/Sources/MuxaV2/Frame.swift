import Foundation

/// Protocol v2 framing (`protocol/PROTOCOL-v2.md`):
/// `[channel u32 LE][kind u8][len u32 LE][payload]`.
public enum FrameKind: UInt8, Sendable {
    case data = 0, credit, close, resize, ack
}

public enum FrameError: Error, Equatable, Sendable {
    case tooLarge(Int)
    case unknownKind(UInt8)
}

public struct Frame: Equatable, Sendable {
    public static let header = 9
    public static let maxPayload = 16 << 20

    public var channel: UInt32
    public var kind: FrameKind
    public var payload: [UInt8]

    public init(channel: UInt32, kind: FrameKind, payload: [UInt8]) {
        self.channel = channel
        self.kind = kind
        self.payload = payload
    }

    public static func control(_ json: Data) -> Frame {
        Frame(channel: 0, kind: .data, payload: [UInt8](json))
    }

    /// `[offset or seq u64 LE][bytes]`: terminal output, or input.
    public static func withU64(_ channel: UInt32, _ n: UInt64, _ bytes: [UInt8]) -> Frame {
        Frame(channel: channel, kind: .data, payload: le(n) + bytes)
    }

    public static func credit(_ channel: UInt32, _ bytes: UInt32) -> Frame {
        Frame(channel: channel, kind: .credit, payload: le(bytes))
    }

    public static func resize(_ channel: UInt32, cols: UInt16, rows: UInt16) -> Frame {
        Frame(channel: channel, kind: .resize, payload: le(cols) + le(rows))
    }

    public func encode() -> [UInt8] {
        le(channel) + [kind.rawValue] + le(UInt32(payload.count)) + payload
    }

    /// Takes one whole frame from the front of `buf`, if it holds one.
    public static func decode(_ buf: inout [UInt8]) throws -> Frame? {
        guard buf.count >= header else { return nil }
        let len = Int(u32(buf, 5))
        guard len <= maxPayload else { throw FrameError.tooLarge(len) }
        guard let kind = FrameKind(rawValue: buf[4]) else { throw FrameError.unknownKind(buf[4]) }
        guard buf.count >= header + len else { return nil }
        let frame = Frame(channel: u32(buf, 0), kind: kind, payload: Array(buf[header..<header + len]))
        buf.removeFirst(header + len)
        return frame
    }

    /// The u64 prefix of an output, input or ack payload, and the rest.
    public func splitU64() -> (UInt64, [UInt8])? {
        guard payload.count >= 8 else { return nil }
        var n: UInt64 = 0
        for i in (0..<8).reversed() { n = n << 8 | UInt64(payload[i]) }
        return (n, Array(payload[8...]))
    }
}

func le<T: FixedWidthInteger>(_ n: T) -> [UInt8] {
    withUnsafeBytes(of: n.littleEndian) { Array($0) }
}

private func u32(_ b: [UInt8], _ at: Int) -> UInt32 {
    UInt32(b[at]) | UInt32(b[at + 1]) << 8 | UInt32(b[at + 2]) << 16 | UInt32(b[at + 3]) << 24
}
