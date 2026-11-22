import Foundation
import Testing

@testable import MuxaV2

struct FrameTests {
    @Test func outputEncodesAsTheRustServerDoes() {
        // muxa_protocol::v2::Frame::output(3, 42, b"hi").encode()
        let bytes = Frame.withU64(3, 42, Array("hi".utf8)).encode()
        #expect(bytes == [3, 0, 0, 0, 0, 10, 0, 0, 0, 42, 0, 0, 0, 0, 0, 0, 0, 104, 105])
        #expect(Frame.resize(1, cols: 80, rows: 24).encode() == [1, 0, 0, 0, 3, 4, 0, 0, 0, 80, 0, 24, 0])
    }

    @Test func framesDecodeOnlyWhenWhole() throws {
        let a = Frame.withU64(3, 42, Array("hi".utf8)).encode()
        let b = Frame(channel: 3, kind: .ack, payload: le(UInt64(7))).encode()
        var buf = Array(a[..<5])
        #expect(try Frame.decode(&buf) == nil)
        buf += a[5...] + b
        let out = try #require(try Frame.decode(&buf))
        #expect(out.splitU64()?.0 == 42)
        #expect(out.splitU64()?.1 == Array("hi".utf8))
        #expect(try Frame.decode(&buf)?.kind == .ack)
        #expect(buf.isEmpty)
    }

    @Test func badFramesAreErrors() {
        var unknown: [UInt8] = [0, 0, 0, 0, 9, 0, 0, 0, 0]
        #expect(throws: FrameError.unknownKind(9)) { _ = try Frame.decode(&unknown) }
        var big: [UInt8] = [0, 0, 0, 0, 0] + le(UInt32(Frame.maxPayload + 1))
        #expect(throws: FrameError.tooLarge(Frame.maxPayload + 1)) { _ = try Frame.decode(&big) }
    }

    @Test func requestsMatchTheSpec() throws {
        let add = try V2Request.inboxAdd(Capture(kind: .reply, title: "bank", due: "2026-12-01")).json(id: 4)
        let json = String(decoding: add, as: UTF8.self)
        #expect(json == #"{"due":"2026-12-01","id":4,"kind":"reply","title":"bank","type":"inbox-add"}"#)
        let start = try V2Request.startTask(workspace: "w", title: "Fix", connections: ["linear"], mcp: ["docs"])
            .json(id: 5)
        let fields = try JSONSerialization.jsonObject(with: start) as? [String: Any]
        #expect(fields?["type"] as? String == "start-task")
        #expect(fields?["connections"] as? [String] == ["linear"])
        #expect(fields?["mcp"] as? [String] == ["docs"])
    }

    @Test func itemsDecodeFromTheWire() throws {
        let wire = #"{"inbox":{"revision":3,"items":[{"id":"i1","kind":"todo","title":"milk","later_until":200,"created":5,"source":"capture"}],"done":[]}}"#
        let inbox: Inbox = try field("inbox", of: Data(wire.utf8))
        #expect(inbox.items[0].laterUntil == 200)
        #expect(inbox.items[0].isLater(now: 100))
    }
}
