import Foundation
import Testing

@testable import MuxaProtocol

/// `protocol/fixtures` is the contract: every line decodes and re-encodes to the
/// same JSON value. Foundation does not keep key order, so compare with sorted keys.
struct FixtureTests {
    static let fixtures = URL(fileURLWithPath: #filePath)
        .deletingLastPathComponent()
        .appendingPathComponent("../../../../protocol/fixtures")
        .standardized

    static func lines(_ name: String) throws -> [Data] {
        let text = try String(contentsOf: fixtures.appendingPathComponent(name), encoding: .utf8)
        return text.split(separator: "\n").map { Data($0.utf8) }
    }

    func canonical(_ json: Data) throws -> String {
        let value = try JSONSerialization.jsonObject(with: json)
        let data = try JSONSerialization.data(
            withJSONObject: value, options: [.sortedKeys, .withoutEscapingSlashes])
        return String(decoding: data, as: UTF8.self)
    }

    @Test func requestsRoundTrip() throws {
        let lines = try Self.lines("requests.jsonl")
        #expect(lines.count == 34)
        for line in lines {
            let request = try Wire.decodeRequest(line)
            let again = try Wire.encode(request)
            #expect(try canonical(again) == canonical(line))
        }
    }

    @Test func messagesRoundTrip() throws {
        let lines = try Self.lines("messages.jsonl")
        #expect(lines.count == 39)
        for line in lines {
            let message = try Wire.decodeMessage(line)
            if case .unknown(let type) = message { Issue.record("fixture type \(type) is unknown") }
            let again = try Wire.encode(message)
            #expect(try canonical(again) == canonical(line))
        }
    }

    @Test func workerContractRoundTripsAWorkerNameOrNone() throws {
        let requests = try Self.lines("workers/requests.jsonl")
        #expect(requests.count == 7)
        for line in requests { #expect(try canonical(Wire.encode(Wire.decodeRequest(line))) == canonical(line)) }
        let messages = try Self.lines("workers/messages.jsonl")
        #expect(messages.count == 5)
        for line in messages { #expect(try canonical(Wire.encode(Wire.decodeMessage(line))) == canonical(line)) }
        let expected: [String?] = [nil, nil, defaultWorker, "Reviewer"]
        for (line, selection) in zip(requests[2...5], expected) {
            guard case .restartAgent(_, _, _, let worker, _) = try Wire.decodeRequest(line) else {
                Issue.record("expected restart"); continue
            }
            #expect(worker == selection)
        }
        for bad in ["true", "17", "{}", "[]"] {
            let line = Data("{\"type\":\"restart-agent\",\"id\":1,\"task\":\"t-000001\",\"worker\":\(bad)}".utf8)
            #expect(throws: (any Error).self) { try Wire.decodeRequest(line) }
        }
        guard case .taskList(let list) = try Wire.decodeMessage(messages[1]) else {
            Issue.record("expected task-list"); return
        }
        // A legacy run has no worker: unrecorded, not the workspace default.
        #expect(list.entries[0].runs.map(\.worker) == [nil, defaultWorker, "Reviewer"])
    }

    @Test func workspaceContractRoundTrips() throws {
        let requests = try Self.lines("workspaces/requests.jsonl")
        #expect(requests.count == 2)
        for line in requests { #expect(try canonical(Wire.encode(Wire.decodeRequest(line))) == canonical(line)) }
        let messages = try Self.lines("workspaces/messages.jsonl")
        #expect(messages.count == 3)
        for line in messages {
            let message = try Wire.decodeMessage(line)
            if case .unknown(let type) = message { Issue.record("fixture type \(type) is unknown") }
            #expect(try canonical(Wire.encode(message)) == canonical(line))
        }
        guard case .hello(_, _, _, let capabilities) = try Wire.decodeMessage(messages[0]) else {
            Issue.record("expected hello"); return
        }
        #expect(workLevel(capabilities) == workAsk)
        guard case .taskList(let listing) = try Wire.decodeMessage(messages[1]) else {
            Issue.record("expected task-list"); return
        }
        #expect(listing.workspaces[0].root == "/work/lisk")
        #expect(listing.workspaces[1].repos.isEmpty)
        guard case .sessionStarted(let start, let session, let name) = try Wire.decodeMessage(messages[2]) else {
            Issue.record("expected session-started"); return
        }
        #expect(start == 41 && session == "$7" && name == "ask-cto-copilot-1")
    }

    @Test func routineContractRoundTrips() throws {
        let requests = try Self.lines("routines/requests.jsonl")
        #expect(requests.count == 6)
        for line in requests { #expect(try canonical(Wire.encode(Wire.decodeRequest(line))) == canonical(line)) }
        let messages = try Self.lines("routines/messages.jsonl")
        #expect(messages.count == 6)
        for line in messages {
            let message = try Wire.decodeMessage(line)
            if case .unknown(let type) = message { Issue.record("fixture type \(type) is unknown") }
            #expect(try canonical(Wire.encode(message)) == canonical(line))
        }
        guard case .hello(_, _, _, let capabilities) = try Wire.decodeMessage(messages[0]) else {
            Issue.record("expected hello"); return
        }
        #expect(workLevel(capabilities) == workRoutines)
        #expect(workLevel(["work:2", "work:x", "workers"]) == 2)
        #expect(workLevel(["task-records"]) == 0)
        guard case .inboxList(_, let entries, let items) = try Wire.decodeMessage(messages[3]) else {
            Issue.record("expected inbox-list"); return
        }
        #expect(entries.map(\.disposition) == ["open", "later"])
        #expect(entries[0].proposal?.hash.hasPrefix("sha256:") == true)
        #expect(items.isEmpty)
    }

    @Test func itemContractRoundTrips() throws {
        let requests = try Self.lines("items/requests.jsonl")
        #expect(requests.count == 16)
        for line in requests { #expect(try canonical(Wire.encode(Wire.decodeRequest(line))) == canonical(line)) }
        let messages = try Self.lines("items/messages.jsonl")
        #expect(messages.count == 4)
        for line in messages {
            let message = try Wire.decodeMessage(line)
            if case .unknown(let type) = message { Issue.record("fixture type \(type) is unknown") }
            #expect(try canonical(Wire.encode(message)) == canonical(line))
        }
        guard case .hello(_, _, _, let capabilities) = try Wire.decodeMessage(messages[0]) else {
            Issue.record("expected hello"); return
        }
        #expect(workLevel(capabilities) == workItems)
        guard case .inboxList(_, let entries, let items) = try Wire.decodeMessage(messages[1]) else {
            Issue.record("expected inbox-list"); return
        }
        #expect(entries.isEmpty)
        #expect(items.map(\.kind) == [.ask, .review, .failure, .proposal, .review, .ask])
        #expect(items.map(\.state) == [.open, .open, .later, .open, .answered, .resolved])
        #expect(items[0].answers == [.approve, .reject, .reply, .later])
        #expect(items[1].answers == [.accept, .reply, .redo, .changeCourse, .keepOpen, .later])
        #expect(items[2].answers == [.retry, .reply, .changeCourse, .drop, .dismiss, .later])
        #expect(!items[3].answers.contains(.reply), "a proposal takes no reply")
        #expect(items[2].attachments.docs == ["report.md"])
        #expect(items[2].attachments.links.map(\.url) == ["https://example.com/runs/42"])
        #expect(items[1].attachments.changes == ChangeCount(files: 4, added: 120, removed: 40, scope: "branch"))
        #expect(items[4].answer == .accept && items[4].answers.isEmpty)
        guard case .inboxSaved(_, nil, let item?) = try Wire.decodeMessage(messages[2]) else {
            Issue.record("expected inbox-saved with an item only"); return
        }
        #expect(item.state == .answered)
        guard case .inboxSaved(_, let entry?, let both?) = try Wire.decodeMessage(messages[3]) else {
            Issue.record("expected inbox-saved with an entry and an item"); return
        }
        #expect(entry.id == both.id)
        guard case .updateInboxEntry(_, _, .choose, nil, nil, 1?) = try Wire.decodeRequest(requests[3]) else {
            Issue.record("expected choose with option 1"); return
        }
        guard case .updateInboxEntry(_, _, .redo, nil, let text?, nil) = try Wire.decodeRequest(requests[6]) else {
            Issue.record("expected redo with text"); return
        }
        #expect(!text.isEmpty)
        let replies = try requests.map(Wire.decodeRequest).filter {
            if case .updateInboxEntry(_, _, .reply, nil, let text?, nil) = $0 { !text.isEmpty } else { false }
        }
        #expect(replies.count == 3)
    }

    @Test func unknownItemKindsStatesAndAnswersStillDecode() throws {
        let json = #"{"id":"i-00000001","kind":"handoff","state":"snoozed","question":"q","workspace":"w","title":"t","#
            + #""createdAt":1,"answer":"escalate","answers":["accept","escalate","later"]}"#
        let item = try JSONDecoder().decode(InboxItem.self, from: Data(json.utf8))
        #expect(item.kind == .other("handoff"))
        #expect(item.state == .other("snoozed"))
        #expect(item.answer == nil)
        #expect(item.answers == [.accept, .later])
        #expect(item.attachments.isEmpty)
    }

    @Test func restartRecoveryRoundTrips() throws {
        for name in ["restart-recovery/requests.jsonl", "restart-recovery/messages.jsonl"] {
            let entries = try Self.lines(name)
            #expect(!entries.isEmpty)
            for line in entries {
                let encoded = name.contains("requests") ? try Wire.encode(Wire.decodeRequest(line)) : try Wire.encode(Wire.decodeMessage(line))
                #expect(try canonical(encoded) == canonical(line))
            }
        }
    }

    @Test func unknownTypesAndFieldsAreIgnored() throws {
        let line = Data(#"{"type":"future-thing","x":1}"#.utf8)
        #expect(try Wire.decodeMessage(line) == .unknown(type: "future-thing"))

        let pane = Data(#"{"type":"pane-gone","watch":3,"pane":"%1","extra":true}"#.utf8)
        #expect(try Wire.decodeMessage(pane) == .paneGone(watch: 3, pane: "%1", reason: nil))
    }

    @Test func newErrorCodesAndRolesStillDecode() throws {
        let line = Data(#"{"type":"result","id":1,"error":{"code":"rate-limited","message":"m"}}"#.utf8)
        guard case .result(_, let error?, _) = try Wire.decodeMessage(line) else {
            Issue.record("expected an error result")
            return
        }
        #expect(error.code == .other("rate-limited"))
        #expect(try JSONDecoder().decode(TaskStatus.self, from: Data(#""blocked""#.utf8)) == .other("blocked"))
        #expect(try JSONDecoder().decode(RunReason.self, from: Data(#""resume""#.utf8)) == .other("resume"))
        #expect(try JSONDecoder().decode(Role.self, from: Data(#""observer""#.utf8)) == .unknown)
        #expect(try JSONDecoder().decode(LoopState.self, from: Data(#""resting""#.utf8)) == .other("resting"))
        #expect(try JSONDecoder().decode(LoopReason.self, from: Data(#""quota""#.utf8)) == .other("quota"))
        #expect(try JSONDecoder().decode(LoopReason.self, from: Data(#""no-agent""#.utf8)) == .noAgent)
    }

    @Test func workspaceChecksAreAnyOrAList() throws {
        let decode = { (json: String) in try JSONDecoder().decode(WorkspaceInfo.self, from: Data(json.utf8)) }
        #expect(try decode(#"{"name":"w","checks":"any"}"#).checks == .any)
        #expect(try decode(#"{"name":"w","checks":["make check"]}"#).checks == .list(["make check"]))
        #expect(try decode(#"{"name":"w"}"#).checks == nil)
        #expect(throws: DecodingError.self) { try decode(#"{"name":"w","checks":"all"}"#) }
    }

    @Test func bytesAreBase64() throws {
        let line = try Wire.encode(Request.bytes(id: 1, pane: "%1", data: Data([0x1b, 0xff])))
        #expect(try canonical(line) == canonical(Data(#"{"type":"bytes","id":1,"pane":"%1","data":"G/8="}"#.utf8)))
    }
}
