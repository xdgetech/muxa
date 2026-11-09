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
        #expect(lines.count == 36)
        for line in lines {
            let request = try Wire.decodeRequest(line)
            let again = try Wire.encode(request)
            #expect(try canonical(again) == canonical(line))
        }
    }

    @Test func messagesRoundTrip() throws {
        let lines = try Self.lines("messages.jsonl")
        #expect(lines.count == 41)
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

    @Test func askCloseContractRoundTrips() throws {
        let requests = try Self.lines("ask-close/requests.jsonl")
        #expect(requests.count == 2)
        for line in requests { #expect(try canonical(Wire.encode(Wire.decodeRequest(line))) == canonical(line)) }
        let messages = try Self.lines("ask-close/messages.jsonl")
        #expect(messages.count == 4)
        for line in messages {
            let message = try Wire.decodeMessage(line)
            if case .unknown(let type) = message { Issue.record("fixture type \(type) is unknown") }
            #expect(try canonical(Wire.encode(message)) == canonical(line))
        }
        guard case .hello(_, _, _, let capabilities) = try Wire.decodeMessage(messages[0]) else {
            Issue.record("expected hello"); return
        }
        #expect(workLevel(capabilities) == workCloseAsk)
        guard case .result(_, _, let sessions?) = try Wire.decodeMessage(messages[1]) else {
            Issue.record("expected sessions"); return
        }
        #expect(sessions.map(\.ask) == [nil, "lisk"])
    }

    @Test func worktreeEndContractRoundTrips() throws {
        let requests = try Self.lines("worktree-end/requests.jsonl")
        #expect(requests.count == 3)
        for line in requests { #expect(try canonical(Wire.encode(Wire.decodeRequest(line))) == canonical(line)) }
        let messages = try Self.lines("worktree-end/messages.jsonl")
        #expect(messages.count == 4)
        for line in messages {
            let message = try Wire.decodeMessage(line)
            if case .unknown(let type) = message { Issue.record("fixture type \(type) is unknown") }
            #expect(try canonical(Wire.encode(message)) == canonical(line))
        }
        guard case .hello(_, _, _, let capabilities) = try Wire.decodeMessage(messages[0]) else {
            Issue.record("expected hello"); return
        }
        #expect(workLevel(capabilities) == workWorktreeEnd)
        guard case .result(_, let error?, _) = try Wire.decodeMessage(messages[1]) else {
            Issue.record("expected an error result"); return
        }
        #expect(error == ErrorBody(code: .wouldLoseWork, message: "its branch is not merged"))
        let ends = try requests.map(Wire.decodeRequest).map { request -> WorktreeEnd? in
            switch request {
            case .updateTask(_, _, _, .done, let end), .updateInboxEntry(_, _, .drop, nil, nil, nil, let end): end
            default: nil
            }
        }
        #expect(ends == [.remove, .force, .remove])
        // Without `worktree` the request is as before: the host keeps the worktree.
        let plain = try Wire.encode(Request.updateTask(id: 1, task: "t-1", rev: 1, status: .done))
        #expect(!String(decoding: plain, as: UTF8.self).contains("worktree"))
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
        guard case .inboxList(_, let entries, let items, _, _, _, _, _) = try Wire.decodeMessage(messages[3]) else {
            Issue.record("expected inbox-list"); return
        }
        #expect(entries.map(\.disposition) == ["open", "later"])
        #expect(entries[0].proposal?.hash.hasPrefix("sha256:") == true)
        #expect(items.isEmpty)
    }

    @Test func itemContractRoundTrips() throws {
        let requests = try Self.lines("items/requests.jsonl")
        #expect(requests.count == 17)
        for line in requests { #expect(try canonical(Wire.encode(Wire.decodeRequest(line))) == canonical(line)) }
        let messages = try Self.lines("items/messages.jsonl")
        #expect(messages.count == 5)
        for line in messages {
            let message = try Wire.decodeMessage(line)
            if case .unknown(let type) = message { Issue.record("fixture type \(type) is unknown") }
            #expect(try canonical(Wire.encode(message)) == canonical(line))
        }
        guard case .hello(_, _, _, let capabilities) = try Wire.decodeMessage(messages[0]) else {
            Issue.record("expected hello"); return
        }
        #expect(workLevel(capabilities) == workItems)
        guard case .inboxList(_, let entries, let items, _, _, _, _, _) = try Wire.decodeMessage(messages[1]) else {
            Issue.record("expected inbox-list"); return
        }
        #expect(entries.isEmpty)
        #expect(items.map(\.kind) == [.ask, .review, .failure, .proposal, .review, .ask])
        #expect(items.map(\.state) == [.open, .open, .open, .open, .answered, .resolved])
        #expect(items[2].laterAt == 1_762_160_500)
        #expect(items[0].answers == [.approve, .reject, .reply, .later])
        #expect(items[1].answers == [.accept, .reply, .keepOpen, .later])
        #expect(items[2].answers == [.retry, .reply, .drop, .dismiss, .later])
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
        guard case .updateInboxEntry(_, _, .choose, nil, nil, 1?, nil) = try Wire.decodeRequest(requests[3]) else {
            Issue.record("expected choose with option 1"); return
        }
        guard case .updateInboxEntry(_, _, .keepOpen, nil, nil, nil, nil) = try Wire.decodeRequest(requests[6]) else {
            Issue.record("expected keep-open"); return
        }
        let replies = try requests.map(Wire.decodeRequest).filter {
            if case .updateInboxEntry(_, _, .reply, nil, let text?, nil, nil) = $0 { !text.isEmpty } else { false }
        }
        #expect(replies.count == 3)
    }

    @Test func assistantContractRoundTrips() throws {
        let requests = try Self.lines("assistant/requests.jsonl")
        #expect(requests.count == 27)
        for line in requests { #expect(try canonical(Wire.encode(Wire.decodeRequest(line))) == canonical(line)) }
        let messages = try Self.lines("assistant/messages.jsonl")
        #expect(messages.count == 22)
        for line in messages {
            let message = try Wire.decodeMessage(line)
            if case .unknown(let type) = message { Issue.record("fixture type \(type) is unknown") }
            #expect(try canonical(Wire.encode(message)) == canonical(line))
        }
        let decoded = try requests.map(Wire.decodeRequest)
        // A resent capture carries the same clientId; the host keeps one.
        let clientIds = decoded[..<2].map { request -> String? in
            if case .capture(_, let clientId, _, _, _, _) = request { clientId } else { nil }
        }
        #expect(clientIds[0] != nil && clientIds[0] == clientIds[1])
        guard case .sortCapture(_, _, _, .link, _?, _, _) = decoded[4] else {
            Issue.record("expected link with a work item"); return
        }
        guard case .updateWork(_, _, _, .merge, _, _, _?, _) = decoded[11] else {
            Issue.record("expected merge with into"); return
        }
        guard case .createTask(_, _, _, _, _, _, _, _, _, _, "w-6a7b8c9d", 3) = decoded[15] else {
            Issue.record("expected create-task with work and workRev"); return
        }
        guard case .hello(_, _, _, let capabilities) = try Wire.decodeMessage(messages[0]) else {
            Issue.record("expected hello"); return
        }
        #expect(workLevel(capabilities) == workAssistant)
        guard case .inboxList(_, _, _, let work, let captures, let listed, _, _) = try Wire.decodeMessage(messages[6]) else {
            Issue.record("expected inbox-list"); return
        }
        #expect(work.map(\.state) == [.waitingDecision, .inProgress, .open, .open, .done, .withdrawn])
        #expect(work[3].laterAt != nil && work[5].mergedInto != nil)
        #expect(captures.count == 1)
        #expect(listed.map(\.delivery) == [.resulted, .processing, .unconfirmed, .timedOut])
        let results = listed[0].results
        #expect(!results[0].stale && results[1].stale)
        #expect(results[0].findings.allSatisfy { !$0.sources.isEmpty })
        // A client before level 8 reads an inbox-list without the new arrays.
        let old = try Wire.decodeMessage(Data(#"{"type":"inbox-list","inbox":1,"entries":[]}"#.utf8))
        guard case .inboxList(_, _, _, let none, let noCaptures, let noRequests, _, _) = old else {
            Issue.record("expected inbox-list"); return
        }
        #expect(none.isEmpty && noCaptures.isEmpty && noRequests.isEmpty)
        guard case .getWork(_, "w-6a7b8c9d") = decoded[16] else {
            Issue.record("expected get-work"); return
        }
        // get-work: every linked Capture, sorted ones included, in the item's order.
        guard case .workDetail(_, let entry, let linked, let asked) = try Wire.decodeMessage(messages[9]) else {
            Issue.record("expected work-detail"); return
        }
        #expect(linked.map(\.id) == entry.captures)
        #expect(linked.allSatisfy { $0.state == .sorted })
        #expect(asked.map(\.id) == entry.requests)
        guard case .result(_, let error?, _) = try Wire.decodeMessage(messages[11]) else {
            Issue.record("expected an error result"); return
        }
        #expect(error.code == .badRequest)
    }

    @Test func planContractRoundTrips() throws {
        let requests = try Self.lines("assistant/requests.jsonl").map(Wire.decodeRequest)
        let messages = try Self.lines("assistant/messages.jsonl").map(Wire.decodeMessage)
        guard case .applyNotes(_, let fromRequest, 1, _) = requests[18], fromRequest.hasPrefix("q-"),
            case .applyNotes(_, let fromPlan, 2, _) = requests[19], fromPlan.hasPrefix("p-")
        else { Issue.record("expected apply-notes from a request and a plan"); return }
        #expect(requests[20] == .planNow(id: 420, minutes: 45))
        #expect(requests[21] == .planNow(id: 421))
        guard case .updateWork(_, _, _, .edit, _, _, _, let due?) = requests[22] else {
            Issue.record("expected edit with due"); return
        }
        #expect(due.timezone == "Asia/Tokyo")
        guard case .updateWork(_, _, _, .pin, _, _, _, _) = requests[23],
            case .updateWork(_, _, _, .unpin, _, _, _, _) = requests[24]
        else { Issue.record("expected pin and unpin"); return }

        guard case .hello(_, _, _, let capabilities) = messages[12] else { Issue.record("expected hello"); return }
        #expect(workLevel(capabilities) == workPlans)
        guard case .workSaved(_, _, let request?) = messages[13], let notes = request.results.first?.notes else {
            Issue.record("expected a result with notes"); return
        }
        #expect(notes.base.count == 64)
        guard case .result(_, let conflict?, _) = messages[14], case .result(_, let unsupported?, _) = messages[16] else {
            Issue.record("expected error results"); return
        }
        #expect(conflict.code == .conflict && unsupported.code == .unsupported)
        guard case .planSaved(420, let pending) = messages[15] else { Issue.record("expected plan-saved"); return }
        #expect(pending.delivery == .pending && pending.minutes == 45 && pending.top.isEmpty)
        guard case .workSaved(_, let pinned, _) = messages[17] else { Issue.record("expected work-saved"); return }
        #expect(pinned.pinnedAt != nil && pinned.due != nil)

        guard case .inboxList(_, _, let items, let work, _, _, let plans, let assistant?) = messages[18] else {
            Issue.record("expected inbox-list with plans"); return
        }
        #expect(items.map(\.kind) == [.sort, .delegate])
        #expect(items[0].sort?.action == .work && items[1].delegate?.scope == .compare)
        #expect(plans.count == 2 && plans[0].at > plans[1].at)
        let plan = plans[0]
        #expect(plan.top.count <= 3)
        #expect(plan.top[0].work == work[0].id && work[0].pinnedAt != nil)
        // A Later item is never in a plan's top; with a near due it is in dueSoon.
        let later = work.filter { $0.laterAt != nil }.map(\.id)
        #expect(plan.top.allSatisfy { row in row.work.map { !later.contains($0) } ?? true })
        #expect(plan.dueSoon == later)
        #expect(plan.top.allSatisfy { ($0.work != nil) != ($0.item != nil) })
        #expect(plan.top.allSatisfy { $0.minutes.low <= $0.minutes.estimated && $0.minutes.estimated <= $0.minutes.high })
        #expect(plan.top[0].basis.first?.kind == .manual)
        #expect(plan.changed.map(\.change) == [.moved, .entered, .entered, .left])
        #expect(!plan.unanswered.isEmpty && !plan.stale.isEmpty)
        #expect(assistant.overall == "secretary")
        #expect(assistant.delegates.map(\.share) == [.summaries, .outline])
        #expect(assistant.delegates.map(\.overallRequests) == [.review, .auto])
        guard case .inboxSaved(_, _, let answered?) = messages[20] else { Issue.record("expected inbox-saved"); return }
        #expect(answered.answers.isEmpty && answered.delegate != nil)
        // A client before level 9 reads an inbox-list without plans.
        let old = try Wire.decodeMessage(Data(#"{"type":"inbox-list","inbox":1,"entries":[]}"#.utf8))
        guard case .inboxList(_, _, _, _, _, _, let none, nil) = old else { Issue.record("expected inbox-list"); return }
        #expect(none.isEmpty)
    }

    @Test func unknownPlanValuesStillDecode() throws {
        let decode = { (json: String) in Data(json.utf8) }
        #expect(try JSONDecoder().decode(BasisKind.self, from: decode(#""habit""#)) == .other("habit"))
        #expect(try JSONDecoder().decode(ChangeKind.self, from: decode(#""split""#)) == .other("split"))
        #expect(try JSONDecoder().decode(Share.self, from: decode(#""full""#)) == .other("full"))
        #expect(try JSONDecoder().decode(OverallRequests.self, from: decode(#""ask""#)) == .other("ask"))
        #expect(try JSONDecoder().decode(ItemKind.self, from: decode(#""sort""#)) == .sort)
    }

    @Test func unknownWorkStatesAndDeliveriesStillDecode() throws {
        let decode = { (json: String) in Data(json.utf8) }
        #expect(try JSONDecoder().decode(WorkState.self, from: decode(#""blocked""#)) == .other("blocked"))
        #expect(try JSONDecoder().decode(Delivery.self, from: decode(#""queued""#)) == .other("queued"))
        #expect(try JSONDecoder().decode(CaptureState.self, from: decode(#""archived""#)) == .other("archived"))
        #expect(try JSONDecoder().decode(CaptureSource.self, from: decode(#""watch""#)) == .other("watch"))
        #expect(try JSONDecoder().decode(AssistScope.self, from: decode(#""summarize""#)) == .other("summarize"))
        #expect(try JSONEncoder().encode(Delivery.other("queued")) == decode(#""queued""#))
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
