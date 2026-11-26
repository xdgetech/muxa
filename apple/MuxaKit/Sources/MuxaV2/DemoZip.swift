import Foundation

/// A stored zip, as the daemon sends a Reader folder (ADR 0101), for the
/// demo's folders. No compression, no time: the demo has no files on disk.
enum DemoZip {
    struct Member {
        let path: String
        let data: Data
    }

    static func stored(_ members: [Member]) -> Data {
        var out = Data()
        var central = Data()
        for m in members {
            let name = Data(m.path.utf8)
            let crc = crc32(m.data)
            let offset = UInt32(out.count)
            // Local header: version 2.0, UTF-8 names, stored, the 1980 epoch.
            out.le(UInt32(0x0403_4B50)); out.le(UInt16(20)); out.le(UInt16(0x0800)); out.le(UInt16(0))
            out.le(UInt16(0)); out.le(UInt16(0x21))
            out.le(crc); out.le(UInt32(m.data.count)); out.le(UInt32(m.data.count))
            out.le(UInt16(name.count)); out.le(UInt16(0))
            out.append(name)
            out.append(m.data)
            central.le(UInt32(0x0201_4B50)); central.le(UInt16(20)); central.le(UInt16(20))
            central.le(UInt16(0x0800)); central.le(UInt16(0)); central.le(UInt16(0)); central.le(UInt16(0x21))
            central.le(crc); central.le(UInt32(m.data.count)); central.le(UInt32(m.data.count))
            central.le(UInt16(name.count)); central.le(UInt16(0)); central.le(UInt16(0))
            central.le(UInt16(0)); central.le(UInt16(0)); central.le(UInt32(0)); central.le(offset)
            central.append(name)
        }
        let start = UInt32(out.count)
        out.append(central)
        out.le(UInt32(0x0605_4B50)); out.le(UInt16(0)); out.le(UInt16(0))
        out.le(UInt16(members.count)); out.le(UInt16(members.count))
        out.le(UInt32(central.count)); out.le(start); out.le(UInt16(0))
        return out
    }

    private static let table: [UInt32] = (0..<256).map { n in
        (0..<8).reduce(UInt32(n)) { c, _ in c & 1 == 1 ? 0xEDB8_8320 ^ (c >> 1) : c >> 1 }
    }

    static func crc32(_ data: Data) -> UInt32 {
        ~data.reduce(~UInt32(0)) { c, byte in table[Int((c ^ UInt32(byte)) & 0xFF)] ^ (c >> 8) }
    }
}

private extension Data {
    mutating func le<T: FixedWidthInteger>(_ value: T) {
        Swift.withUnsafeBytes(of: value.littleEndian) { append(contentsOf: $0) }
    }
}
