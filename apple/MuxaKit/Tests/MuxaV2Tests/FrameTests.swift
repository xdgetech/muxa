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
        #expect(try wire(.inboxDeleteOlder(days: 3)) == #"{"days":3,"id":1,"type":"inbox-delete-older"}"#)
    }

    @Test func anExistingPlaceMatchesTheSpec() throws {
        let start = try V2Request.startTask(workspace: "app", title: "Tidy", places: [.existing(path: "/w/tasks/tidy")])
            .json(id: 2)
        let fields = try JSONSerialization.jsonObject(with: start) as? [String: Any]
        #expect(fields?["places"] as? [[String: String]] == [["kind": "existing", "path": "/w/tasks/tidy"]])
    }

    @Test func aMigratedItemNamesItsPlace() throws {
        let wire = #"{"inbox":{"revision":1,"items":[{"id":"i1","kind":"todo","title":"Resume: Tidy","created":1,"source":"v1","place":{"workspace":"app","path":"/w/tasks/tidy"}},{"id":"i2","kind":"todo","title":"c","created":1,"source":"capture"}],"done":[]}}"#
        let items = try (field("inbox", of: Data(wire.utf8)) as Inbox).items
        #expect(items[0].place == ItemPlace(workspace: "app", path: "/w/tasks/tidy"))
        #expect(items[1].place == nil)
        let back = try JSONSerialization.jsonObject(with: JSONEncoder().encode(items[1])) as? [String: Any]
        #expect(back?["place"] == nil, "no place is absent on the wire")
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

    @Test func readerEntriesSayWhenGitIgnoresThem() throws {
        let wire = #"{"entries":[{"path":"README.md"},{"path":"output/qa/report.md","ignored":true}]}"#
        let entries: [ReaderEntry] = try field("entries", of: Data(wire.utf8))
        #expect(entries == [ReaderEntry(path: "README.md"), ReaderEntry(path: "output/qa/report.md", ignored: true)])
    }

    @Test func connectionsAndServersSayWhoUsesThem() throws {
        let wire = #"{"workspaces":[{"name":"a","root":"/a","repos":[],"loop":null,"connections":[{"name":"github","default":false,"used_by":{"routines":["digest"],"tasks":["t4"]},"last_used":1700000000}],"mcp":[{"name":"docs","default":true,"used_by":{"routines":[],"tasks":[]},"last_used":null}]}]}"#
        let ws: [Workspace] = try field("workspaces", of: Data(wire.utf8))
        #expect(ws[0].connections == [WorkspaceConnection(
            name: "github", isDefault: false, usedBy: UsedBy(routines: ["digest"], tasks: ["t4"]), lastUsed: 1_700_000_000)])
        #expect(ws[0].mcp == [WorkspaceMcp(name: "docs", isDefault: true)])
        let e = JSONEncoder()
        e.keyEncodingStrategy = .convertToSnakeCase
        let back = try JSONSerialization.jsonObject(with: e.encode(ws[0].mcp[0])) as? [String: Any]
        #expect(back?["last_used"] is NSNull, "never used is null on the wire")
        #expect(back?["used_by"] != nil)
    }

    @Test func itemsDecodeFromTheWire() throws {
        // An older server may still send `later_until`; it is ignored (ADR 0096).
        let wire = #"{"inbox":{"revision":3,"items":[{"id":"i1","kind":"todo","title":"milk","later_until":200,"created":5,"source":"capture"}],"done":[]}}"#
        let inbox: Inbox = try field("inbox", of: Data(wire.utf8))
        #expect(inbox.items[0].title == "milk")
        #expect(inbox.items[0].links == nil && inbox.bots == nil)
    }

    @Test func laterSendsNoTime() throws {
        let frame = try JSONSerialization.jsonObject(with: V2Request.inboxLater(item: "i1").json(id: 4))
            as? [String: Any]
        #expect(frame?["type"] as? String == "inbox-later")
        #expect(frame?["until"] == nil)
    }

    @Test func aBotItemDecodesWithItsLinksAndProposal() throws {
        let wire = #"{"inbox":{"revision":1,"items":[{"id":"i2","kind":"decide","title":"Comment on PR 7","created":5,"source":"routine","workspace":"w","bot":"pr-watch","links":[{"url":"https://github.com/o/r/pull/7","title":"PR 7"}],"proposal":{"action":"comment","target":"o/r#7","content":"LGTM"}}],"done":[],"bots":[{"workspace":"w","bot":"pr-watch","done":4,"deleted":1}]}}"#
        let inbox: Inbox = try field("inbox", of: Data(wire.utf8))
        let item = inbox.items[0]
        #expect(item.bot == "pr-watch")
        #expect(item.links == [ItemLink(url: "https://github.com/o/r/pull/7", title: "PR 7")])
        #expect(item.proposal == ItemProposal(action: "comment", target: "o/r#7", content: "LGTM"))
        #expect(inbox.bots == [BotCount(workspace: "w", bot: "pr-watch", done: 4, deleted: 1)])
    }
}
