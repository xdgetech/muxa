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
        #expect(requests.count == 3)
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
        guard case .startRootSession(43, "secretary") = try Wire.decodeRequest(requests[2]) else {
            Issue.record("expected start-session with root"); return
        }
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
            case .updateTask(_, _, _, .done, let end), .updateInboxEntry(_, _, .drop, nil, nil, nil, let end, _): end
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
        guard case .inboxList(_, let entries, let items, _, _, _, _, _, _, _, _) = try Wire.decodeMessage(messages[3]) else {
            Issue.record("expected inbox-list"); return
        }
        #expect(entries.map(\.disposition) == ["open", "later"])
        #expect(entries[0].proposal?.hash.hasPrefix("sha256:") == true)
        #expect(items.isEmpty)
    }

    @Test func itemContractRoundTrips() throws {
        let requests = try Self.lines("items/requests.jsonl")
        #expect(requests.count == 23)
        for line in requests { #expect(try canonical(Wire.encode(Wire.decodeRequest(line))) == canonical(line)) }
        let messages = try Self.lines("items/messages.jsonl")
        #expect(messages.count == 14)
        for line in messages {
            let message = try Wire.decodeMessage(line)
            if case .unknown(let type) = message { Issue.record("fixture type \(type) is unknown") }
            #expect(try canonical(Wire.encode(message)) == canonical(line))
        }
        guard case .hello(_, _, _, let capabilities) = try Wire.decodeMessage(messages[0]) else {
            Issue.record("expected hello"); return
        }
        #expect(workLevel(capabilities) == workItems)
        guard case .inboxList(_, let entries, let items, _, _, _, _, _, _, _, _) = try Wire.decodeMessage(messages[1]) else {
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
        guard case .updateInboxEntry(_, _, .choose, nil, nil, 1?, nil, _) = try Wire.decodeRequest(requests[3]) else {
            Issue.record("expected choose with option 1"); return
        }
        guard case .updateInboxEntry(_, _, .keepOpen, nil, nil, nil, nil, _) = try Wire.decodeRequest(requests[6]) else {
            Issue.record("expected keep-open"); return
        }
        let replies = try requests.map(Wire.decodeRequest).filter {
            if case .updateInboxEntry(_, _, .reply, nil, let text?, nil, nil, _) = $0 { !text.isEmpty } else { false }
        }
        #expect(replies.count == 3)
    }

    @Test func assistantContractRoundTrips() throws {
        let requests = try Self.lines("assistant/requests.jsonl")
        #expect(requests.count == 49)
        for line in requests { #expect(try canonical(Wire.encode(Wire.decodeRequest(line))) == canonical(line)) }
        let messages = try Self.lines("assistant/messages.jsonl")
        #expect(messages.count == 58)
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
        guard case .inboxList(_, _, _, let work, let captures, let listed, _, _, _, _, _) = try Wire.decodeMessage(messages[6]) else {
            Issue.record("expected inbox-list"); return
        }
        #expect(work.map(\.state) == [.waitingDecision, .inProgress, .open, .open, .done, .withdrawn])
        #expect(work[3].laterAt != nil && work[5].mergedInto != nil)
        #expect(captures.count == 1)
        #expect(listed.map(\.delivery) == [.resulted, .processing, .unconfirmed, .timedOut])
        let results = listed[0].results
        #expect(!results[0].stale && results[1].stale)
        #expect(results[0].findings.allSatisfy { !$0.sources.isEmpty })
        // A client before level 9 reads an inbox-list without the new arrays.
        let old = try Wire.decodeMessage(Data(#"{"type":"inbox-list","inbox":1,"entries":[]}"#.utf8))
        guard case .inboxList(_, _, _, let none, let noCaptures, let noRequests, _, _, _, _, _) = old else {
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
        guard case .applyNotes(_, let fromRequest, 1, _, nil) = requests[18], fromRequest.hasPrefix("q-"),
            case .applyNotes(_, let fromPlan, 2, _, nil) = requests[19], fromPlan.hasPrefix("p-")
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

        guard case .inboxList(_, _, let items, let work, _, _, let plans, let assistant?, _, _, _) = messages[18] else {
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
        // A plan whose run did not start says why, in host text.
        guard case .planSaved(_, let failed) = messages[22] else { Issue.record("expected plan-saved"); return }
        #expect(failed.delivery == .failed && (failed.reason?.utf8.count ?? 0) > 0)
        #expect(plans.allSatisfy { $0.reason == nil })
        // A client before level 10 reads an inbox-list without plans.
        let old = try Wire.decodeMessage(Data(#"{"type":"inbox-list","inbox":1,"entries":[]}"#.utf8))
        guard case .inboxList(_, _, _, _, _, _, let none, nil, _, _, _) = old else { Issue.record("expected inbox-list"); return }
        #expect(none.isEmpty)
    }

    @Test func policyContractRoundTrips() throws {
        let requests = try Self.lines("assistant/requests.jsonl").map(Wire.decodeRequest)
        let messages = try Self.lines("assistant/messages.jsonl").map(Wire.decodeMessage)
        guard case .addPolicy(_, _, "all", nil, _?) = requests[27] else { Issue.record("expected add-policy"); return }
        guard case .updatePolicy(_, _, _, .edit, _?, _, _, _) = requests[29],
            case .updatePolicy(_, _, _, .delete, nil, nil, nil, nil) = requests[30]
        else { Issue.record("expected edit and delete"); return }
        guard case .updatePlan(_, _, _, .reorder, let order, nil, nil) = requests[31],
            case .updatePlan(_, _, _, .dismiss, [], _?, _?) = requests[32]
        else { Issue.record("expected reorder and dismiss"); return }
        #expect(order.count == 3)
        guard case .updateInboxEntry(_, _, .reject, _, _?, _, _, _) = requests[34],
            case .updateInboxEntry(_, _, .confirm, _, nil, _, _, _) = requests[35]
        else { Issue.record("expected reject with a reason and confirm"); return }
        #expect(requests[38] == .forget(id: 441, record: "h-3c4d5e6f"))

        guard case .hello(_, _, _, let capabilities) = messages[23] else { Issue.record("expected hello"); return }
        #expect(workLevel(capabilities) == workPolicies)
        guard case .policySaved(430, let added) = messages[24] else { Issue.record("expected policy-saved"); return }
        #expect(added.id.hasPrefix("y-") && added.source == "user" && added.until != nil)
        guard case .result(_, let conflict?, _) = messages[26], case .result(433, nil, _) = messages[28] else {
            Issue.record("expected the limit and a delete's bare result"); return
        }
        #expect(conflict.code == .conflict)
        guard case .planSaved(_, let plan) = messages[29] else { Issue.record("expected plan-saved"); return }
        #expect(plan.userOrder.count == plan.top.count && plan.dismissed == ["i-1a2b3c4d"])
        // The host names each row and says which target is no longer open.
        let answered = plan.top.first { $0.item == "i-1a2b3c4d" }
        #expect(answered?.targetState == .answered && answered?.title != nil)
        #expect(plan.top.filter { $0.work != nil }.allSatisfy { $0.title != nil && $0.targetState == nil })
        let kinds = Set(plan.top.flatMap { $0.basis.map(\.kind) })
        #expect(kinds.isSuperset(of: [.policy, .hypothesis, .correction]))
        guard case .policySaved(_, let made) = messages[32], case .inboxSaved(_, _, let confirmed?) = messages[33] else {
            Issue.record("expected confirm's policy and item"); return
        }
        #expect(made.source == confirmed.hypothesis?.hypothesis && confirmed.answer == .confirm)
        guard case .result(_, let forgot?, _) = messages[35] else { Issue.record("expected an error"); return }
        #expect(forgot.code == .badRequest)

        guard case .inboxList(_, _, let items, _, _, _, _, _, let policies, let hypotheses, let corrections) = messages[36]
        else { Issue.record("expected inbox-list"); return }
        #expect(items[0].kind == .hypothesis && items[0].answers == [.confirm, .reject, .later])
        #expect(items[0].hypothesis?.confidence == .low)
        #expect(policies.count == 3)
        #expect(hypotheses.map(\.state) == [.proposed, .rejected])
        #expect(corrections.map(\.kind) == [.dismissed, .reordered, .rejected, .deferred, .pinned])
        #expect(corrections[1].row == nil && corrections[1].before.count == corrections[1].after.count)
        #expect(corrections[3].reason == nil && !corrections[3].inTop)
    }

    @Test func staffContractRoundTrips() throws {
        let requests = try Self.lines("assistant/requests.jsonl").map(Wire.decodeRequest)
        let messages = try Self.lines("assistant/messages.jsonl").map(Wire.decodeMessage)
        guard case .requestAssistant(_, _, _, _, _, _, "lisk-strategist") = requests[39] else {
            Issue.record("expected request-assistant with a worker"); return
        }
        guard case .capture(_, _, _, _, _, let assist?) = requests[40] else { Issue.record("expected capture"); return }
        #expect(assist.worker == "lisk-architect")
        guard case .applyNotes(_, _, _, _, 2) = requests[42], case .applyNotes(_, _, _, _, nil) = requests[18] else {
            Issue.record("expected apply-notes with a step, and an older one without"); return
        }

        guard case .hello(_, _, _, let capabilities) = messages[37] else { Issue.record("expected hello"); return }
        #expect(workLevel(capabilities) == workStaff)
        // A named worker is step 1, picked by the user.
        guard case .workSaved(_, _, let asked?) = messages[38] else { Issue.record("expected work-saved"); return }
        #expect(asked.steps.count == 1 && asked.steps[0].by == .user && asked.steps[0].run == asked.run)
        // Approving a hand-off: the request with its new step, then the answered item.
        guard case .workSaved(_, _, let chain?) = messages[42], let newest = chain.steps.last else {
            Issue.record("expected work-saved with steps"); return
        }
        #expect(newest.step == 2 && newest.by == .handoff && newest.delivery == chain.delivery)
        #expect(newest.run == chain.run && chain.steps[0].handoff?.worker == newest.worker)
        guard case .inboxSaved(_, _, let item?) = messages[43], let handoff = item.handoff else {
            Issue.record("expected the answered handoff item"); return
        }
        #expect(item.kind == .handoff && item.answer == .approve && handoff.step == 1 && item.answers.isEmpty)
        // Each result names its worker and step; inside a version nothing is stale.
        guard case .workSaved(_, _, let done?) = messages[45] else { Issue.record("expected work-saved"); return }
        #expect(done.delivery == .resulted)
        let current = done.results.filter { $0.version == done.version }
        #expect(current.map(\.step) == [2, 1] && current.allSatisfy { !$0.stale })
        #expect(done.results[0].notes != nil && done.results[0].worker != nil)
        guard case .result(_, let refused?, _) = messages[47] else { Issue.record("expected an error"); return }
        #expect(refused.code == .badRequest)

        guard case .inboxList(_, _, let items, _, _, _, _, let assistant?, _, _, _) = messages[48] else {
            Issue.record("expected inbox-list"); return
        }
        #expect(items.map(\.kind) == [.handoff, .delegate])
        #expect(items[0].answers == [.approve, .reject, .later])
        #expect(items[1].delegate?.worker == "lisk-security")
        let staff = assistant.delegates[0].staff
        #expect(staff.filter(\.lead).count == 1)
        #expect(staff.allSatisfy { $0.description.utf8.count <= 1024 })
        // A client before level 12 reads an inbox-list without staff.
        let old = try Wire.decodeMessage(
            Data(
                #"{"type":"inbox-list","inbox":1,"entries":[],"assistant":{"delegates":[{"workspace":"w","share":"outline","overallRequests":"review"}]}}"#
                    .utf8))
        guard case .inboxList(_, _, _, _, _, _, _, let oldAssistant?, _, _, _) = old else {
            Issue.record("expected inbox-list"); return
        }
        #expect(oldAssistant.delegates[0].staff.isEmpty)
    }

    /// A level 11 client knows no steps, staff or result worker: it reads the newest
    /// step as the request and ignores the rest.
    @Test func aLevel11ClientReadsAChainAsOneRequest() throws {
        struct OldResult: Decodable { var version: UInt64; var summary: String }
        struct OldRequest: Decodable { var delivery: Delivery; var run: String?; var results: [OldResult] }
        struct OldSaved: Decodable { var request: OldRequest }
        let line = try Self.lines("assistant/messages.jsonl")[45]
        let old = try JSONDecoder().decode(OldSaved.self, from: line).request
        #expect(old.delivery == .resulted && old.run == "t-00e004")
        #expect(old.results.allSatisfy { $0.version >= 1 && !$0.summary.isEmpty })
        // A newer host's `by` is unknown, not a failed list.
        var list = try #require(
            JSONSerialization.jsonObject(with: Self.lines("assistant/messages.jsonl")[48]) as? [String: Any])
        var requests = try #require(list["requests"] as? [[String: Any]])
        var steps = try #require(requests[0]["steps"] as? [[String: Any]])
        steps[0]["by"] = "routine"
        requests[0]["steps"] = steps
        list["requests"] = requests
        guard case .inboxList(_, _, _, _, _, let listed, _, _, _, _, _) = try Wire.decodeMessage(
            JSONSerialization.data(withJSONObject: list))
        else { Issue.record("expected inbox-list"); return }
        #expect(listed[0].steps[0].by == .other("routine"))
    }

    @Test func writeContractRoundTrips() throws {
        let requests = try Self.lines("assistant/requests.jsonl").map(Wire.decodeRequest)
        let messages = try Self.lines("assistant/messages.jsonl").map(Wire.decodeMessage)
        guard case .updateInboxEntry(_, "i-9c0d1e2f", .approve, _, _, _, _, _) = requests[44],
            case .updateInboxEntry(_, _, .reject, _, _, _, _, _) = requests[46]
        else { Issue.record("expected approve and reject on write items"); return }

        guard case .hello(_, _, _, let capabilities) = messages[49] else { Issue.record("expected hello"); return }
        #expect(workLevel(capabilities) == workWrites)
        // Approve: the writer's Task, then the answered item that names it.
        guard case .taskSaved(_, let record) = messages[50], case .inboxSaved(_, _, let item?) = messages[51],
            let write = item.write
        else { Issue.record("expected the writer's Task and the answered item"); return }
        #expect(item.kind == .write && item.answer == .approve && item.task == record.id)
        #expect(record.worker == "lisk-writer" && record.title == write.action)
        // The content is in the goal unchanged, and the marker is the last line.
        #expect(record.goal.contains("\n\n\(write.content)\n\n"))
        let last = record.goal.split(separator: "\n").last.map(String.init)
        #expect(last == "muxa-write: \(item.id) \(write.hash)")
        guard case .result(_, let refused?, _) = messages[53] else { Issue.record("expected an error"); return }
        #expect(refused.code == .conflict && refused.message.contains("max_writes_per_day"))
        guard case .inboxSaved(_, _, let rejected?) = messages[54] else { Issue.record("expected inbox-saved"); return }
        #expect(rejected.answer == .reject && rejected.task == nil)

        guard case .inboxList(_, _, let items, _, _, let listed, _, let assistant?, _, _, _) = messages[56] else {
            Issue.record("expected inbox-list"); return
        }
        #expect(items.allSatisfy { $0.kind == .write && $0.state == .open })
        #expect(items[0].answers == [.approve, .reject, .later])
        // An older version's write stays open and names its version.
        let q = listed[0]
        #expect(items[1].write?.version == q.version - 1)
        // A Result's writes carry their hash, the item, and the Task once approved.
        let newest = q.results[0]
        #expect(newest.writes.count == 2 && newest.writes[0].task == record.id)
        #expect(newest.writes[1].item == items[0].id && newest.writes[1].hash == items[0].write?.hash)
        let lisk = assistant.delegates[0]
        #expect(lisk.writer?.worker == "lisk-writer" && lisk.writes == Writes.review)
        #expect(!lisk.staff.contains { $0.worker == "lisk-writer" })
        let personal = assistant.delegates[1]
        #expect(personal.writer == nil && personal.writes == Writes.none)
        // A host before level 13 sends neither.
        guard case .inboxList(_, _, _, _, _, _, _, let older?, _, _, _) = messages[48] else {
            Issue.record("expected inbox-list"); return
        }
        #expect(older.delegates.allSatisfy { $0.writer == nil && $0.writes == nil })
        // Approve on a write carries the hash of the write the client showed; without it, refused.
        guard case .updateInboxEntry(_, let entry, .approve, _, _, _, _, let shown?) = requests[47],
            case .updateInboxEntry(_, _, .approve, _, _, _, _, nil) = requests[48],
            case .result(_, let old?, _) = messages[57]
        else { Issue.record("expected approves with and without a hash"); return }
        #expect(items.first { $0.id == entry }?.write?.hash == shown)
        #expect(old.code == .badRequest && old.message.contains("cannot show the whole write"))
    }

    /// A level 12 client knows no writes: it reads the Result and shows a `write` item as
    /// a kind it does not know.
    @Test func aLevel12ClientReadsWritesAsUnknown() throws {
        struct OldItem: Decodable { var kind: String }
        struct OldResult: Decodable { var version: UInt64; var summary: String }
        struct OldRequest: Decodable { var results: [OldResult] }
        struct OldList: Decodable { var items: [OldItem]; var requests: [OldRequest] }
        let line = try Self.lines("assistant/messages.jsonl")[56]
        let old = try JSONDecoder().decode(OldList.self, from: line)
        let level12: Set<String> = ["ask", "review", "failure", "proposal", "briefing", "sort", "delegate", "hypothesis", "handoff"]
        #expect(old.items.allSatisfy { !level12.contains($0.kind) })
        #expect(old.requests[0].results.allSatisfy { $0.version >= 1 && !$0.summary.isEmpty })
        // A newer host's `writes` value is unknown, not a failed list.
        let newer = String(decoding: line, as: UTF8.self).replacingOccurrences(
            of: #""writes":"review""#, with: #""writes":"auto""#)
        guard case .inboxList(_, _, _, _, _, _, _, let assistant?, _, _, _) = try Wire.decodeMessage(Data(newer.utf8))
        else { Issue.record("expected inbox-list"); return }
        #expect(assistant.delegates[0].writes == .other("auto"))
    }

    /// ADR 0053: `memory` items, approve with the shown hash, and a workspace's `memory`
    /// mode, from line 6 of `items/messages.jsonl`.
    @Test func memoryContractRoundTrips() throws {
        let requests = try Self.lines("items/requests.jsonl").map(Wire.decodeRequest)
        let messages = try Self.lines("items/messages.jsonl").map(Wire.decodeMessage)
        guard case .hello(_, _, _, let capabilities) = messages[5] else { Issue.record("expected hello"); return }
        #expect(workLevel(capabilities) == workMemory)
        guard case .taskList(let listing) = messages[6] else { Issue.record("expected task-list"); return }
        #expect(listing.workspaces.map(\.memory) == [.review, .auto, .off])
        #expect(listing.workspaces.map(\.memoryAt) == [1_762_790_000, nil, nil])
        guard case .inboxList(_, _, let items, _, _, _, _, _, _, _, _) = messages[7] else {
            Issue.record("expected inbox-list"); return
        }
        for item in items {
            #expect(item.kind == .memory && item.state == .open && item.task == item.memory?.task)
            #expect(item.title == item.memory?.title)
            #expect(item.answers == [.approve, .reject, .later])
        }
        // Approve carries the hash of the text the client showed.
        guard case .updateInboxEntry(_, let entry, .approve, _, _, _, _, let shown?) = requests[19],
            let approved = items.first(where: { $0.id == entry })
        else { Issue.record("expected approve with a hash"); return }
        #expect(shown == approved.memory?.hash)
        guard case .inboxSaved(_, _, let saved?) = messages[8] else { Issue.record("expected inbox-saved"); return }
        #expect(saved.state == .answered && saved.answer == .approve && saved.memory == approved.memory)
        // Over the file's cap: conflict with the sizes, and the item stays open.
        guard case .result(_, let full?, _) = messages[10] else { Issue.record("expected an error"); return }
        #expect(full.code == .conflict && full.message.contains("16 KiB"))
        // Without the hash: refused.
        guard case .updateInboxEntry(_, _, .approve, _, _, _, _, nil) = requests[21] else {
            Issue.record("expected approve without a hash"); return
        }
        guard case .result(_, let refused?, _) = messages[11] else { Issue.record("expected an error"); return }
        #expect(refused.code == .badRequest)
        guard case .inboxSaved(_, _, let rejected?) = messages[12] else { Issue.record("expected inbox-saved"); return }
        #expect(rejected.answer == .reject)
        // A host before level 14 sends no mode; a newer value is unknown.
        let decode = { (json: String) in try JSONDecoder().decode(WorkspaceInfo.self, from: Data(json.utf8)) }
        let old = try decode(#"{"name":"w"}"#)
        #expect(old.memory == nil && old.memoryAt == nil)
        #expect(try decode(#"{"name":"w","memory":"ask"}"#).memory == .other("ask"))
    }

    /// A level 13 client knows no memory: it shows a `memory` item as a kind it does not
    /// know, and reads the workspaces.
    @Test func aLevel13ClientReadsMemoryAsUnknown() throws {
        struct OldItem: Decodable { var kind: String; var title: String }
        struct OldList: Decodable { var items: [OldItem] }
        struct OldWorkspace: Decodable { var name: String; var root: String? }
        struct OldTaskList: Decodable { var workspaces: [OldWorkspace] }
        let lines = try Self.lines("items/messages.jsonl")
        let old = try JSONDecoder().decode(OldList.self, from: lines[7])
        let level13: Set<String> = [
            "ask", "review", "failure", "proposal", "briefing", "sort", "delegate", "hypothesis", "handoff", "write",
        ]
        #expect(old.items.allSatisfy { !level13.contains($0.kind) && !$0.title.isEmpty })
        let workspaces = try JSONDecoder().decode(OldTaskList.self, from: lines[6]).workspaces
        #expect(workspaces.allSatisfy { !$0.name.isEmpty && $0.root != nil })
    }

    @Test func unknownPolicyValuesStillDecode() throws {
        let decode = { (json: String) in Data(json.utf8) }
        #expect(try JSONDecoder().decode(CorrectionKind.self, from: decode(#""skipped""#)) == .other("skipped"))
        #expect(try JSONDecoder().decode(Confidence.self, from: decode(#""high""#)) == .other("high"))
        #expect(try JSONDecoder().decode(HypothesisState.self, from: decode(#""paused""#)) == .other("paused"))
        #expect(try JSONDecoder().decode(ItemKind.self, from: decode(#""hypothesis""#)) == .hypothesis)
        #expect(try JSONDecoder().decode(BasisKind.self, from: decode(#""policy""#)) == .policy)
    }

    @Test func unknownPlanValuesStillDecode() throws {
        let decode = { (json: String) in Data(json.utf8) }
        #expect(try JSONDecoder().decode(BasisKind.self, from: decode(#""habit""#)) == .other("habit"))
        #expect(try JSONDecoder().decode(ChangeKind.self, from: decode(#""split""#)) == .other("split"))
        #expect(try JSONDecoder().decode(TargetState.self, from: decode(#""archived""#)) == .other("archived"))
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

    @Test func deleteInboxContractRoundTrips() throws {
        let requests = try Self.lines("inbox-delete/requests.jsonl")
        #expect(requests.count == 5)
        for line in requests { #expect(try canonical(Wire.encode(Wire.decodeRequest(line))) == canonical(line)) }
        let messages = try Self.lines("inbox-delete/messages.jsonl")
        #expect(messages.count == 6)
        for line in messages {
            let message = try Wire.decodeMessage(line)
            if case .unknown(let type) = message { Issue.record("fixture type \(type) is unknown") }
            #expect(try canonical(Wire.encode(message)) == canonical(line))
        }
        guard case .hello(_, _, _, let capabilities) = try Wire.decodeMessage(messages[0]) else {
            Issue.record("expected hello"); return
        }
        #expect(workLevel(capabilities) == workDeleteInbox)
        guard case .inboxSaved(_, nil, let item?) = try Wire.decodeMessage(messages[1]) else {
            Issue.record("expected inbox-saved with an item only"); return
        }
        #expect(item.protectedAt == 1_762_160_000)
        #expect(!item.answers.contains(.protect), "protect is never an answer")
        guard case .inboxSaved(_, let entry?, let both?) = try Wire.decodeMessage(messages[2]) else {
            Issue.record("expected inbox-saved with an entry and an item"); return
        }
        #expect(entry.protectedAt == 1_762_160_000 && both.protectedAt == 1_762_160_000)
        guard case .inboxDeleted(403, 12) = try Wire.decodeMessage(messages[3]) else {
            Issue.record("expected inbox-deleted 12"); return
        }
        guard case .updateInboxEntry(_, _, .protect, nil, nil, nil, nil, _) = try Wire.decodeRequest(requests[0]),
              case .updateInboxEntry(_, _, .unprotect, nil, nil, nil, nil, _) = try Wire.decodeRequest(requests[2]),
              case .deleteInbox(403, 3) = try Wire.decodeRequest(requests[3])
        else {
            Issue.record("expected protect, unprotect and delete-inbox 3"); return
        }
    }

    @Test func unknownItemKindsStatesAndAnswersStillDecode() throws {
        let json = #"{"id":"i-00000001","kind":"escalation","state":"snoozed","question":"q","workspace":"w","title":"t","#
            + #""createdAt":1,"answer":"escalate","answers":["accept","escalate","later"]}"#
        let item = try JSONDecoder().decode(InboxItem.self, from: Data(json.utf8))
        #expect(item.kind == .other("escalation"))
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
