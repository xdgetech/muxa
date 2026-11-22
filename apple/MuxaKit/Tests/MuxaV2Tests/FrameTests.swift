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

    @Test func taskActionsMatchTheSpec() throws {
        func wire(_ r: V2Request) throws -> String { String(decoding: try r.json(id: 1), as: UTF8.self) }
        #expect(try wire(.inboxApprove(item: "i3")) == #"{"id":1,"item":"i3","type":"inbox-approve"}"#)
        #expect(try wire(.inboxApprove(item: "i3", worktree: .force))
            == #"{"id":1,"item":"i3","type":"inbox-approve","worktree":"force"}"#)
        #expect(try wire(.inboxRequestChanges(item: "i3", comment: "more"))
            == #"{"comment":"more","id":1,"item":"i3","type":"inbox-request-changes"}"#)
        #expect(try wire(.inboxRetry(item: "i3")) == #"{"id":1,"item":"i3","type":"inbox-retry"}"#)
        #expect(try wire(.workspaces) == #"{"id":1,"type":"workspaces"}"#)
    }

    @Test func itemsSayWhatMadeThem() throws {
        let wire = #"{"inbox":{"revision":1,"items":[{"id":"i1","kind":"review","title":"a","created":1,"source":"task","terminal":"t4"},{"id":"i2","kind":"failed","title":"b","created":1,"source":"routine","routine":{"workspace":"app","name":"digest"}},{"id":"i3","kind":"todo","title":"c","created":1,"source":"capture"}],"done":[]}}"#
        let items = try (field("inbox", of: Data(wire.utf8)) as Inbox).items
        #expect(items[0].terminal == "t4" && items[0].routine == nil)
        #expect(items[1].routine == RoutineRef(workspace: "app", name: "digest") && items[1].terminal == nil)
        #expect(items[2].terminal == nil && items[2].routine == nil)
    }

    @Test func workspacesDecodeEachLoopShape() throws {
        let wire = #"{"workspaces":[{"name":"a","root":"/a","repos":["app"],"agent":"claude","loop":{"checks":"any"},"connections":[{"name":"github","default":true}]},{"name":"b","root":"/b","repos":[],"loop":{"checks":["make test"]},"connections":[]},{"name":"c","root":"/c","repos":[],"loop":null,"connections":[]}]}"#
        let ws: [Workspace] = try field("workspaces", of: Data(wire.utf8))
        #expect(ws[0] == Workspace(name: "a", root: "/a", repos: ["app"], agent: "claude", loop: WorkspaceLoop(checks: .any),
                                   connections: [WorkspaceConnection(name: "github", isDefault: true)]))
        #expect(ws[1].loop?.checks == .list(["make test"]) && ws[1].agent == nil)
        #expect(ws[2].loop == nil)
        let bad = #"{"workspaces":[{"name":"d","root":"/d","repos":[],"loop":{"checks":"all"},"connections":[]}]}"#
        #expect(throws: (any Error).self) { let _: [Workspace] = try field("workspaces", of: Data(bad.utf8)) }
    }

    @Test func itemsDecodeFromTheWire() throws {
        let wire = #"{"inbox":{"revision":3,"items":[{"id":"i1","kind":"todo","title":"milk","later_until":200,"created":5,"source":"capture"}],"done":[]}}"#
        let inbox: Inbox = try field("inbox", of: Data(wire.utf8))
        #expect(inbox.items[0].laterUntil == 200)
        #expect(inbox.items[0].isLater(now: 100))
    }
}
