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
